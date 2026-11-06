//! Snapshot-backed fixture bounds are local to these controller tests; they are
//! not campaign, replay-promotion, or release evidence.
use super::*;
use chaoscontrol_fault::mechanisms::{NetworkDirection, NetworkDisruption};
use chaoscontrol_fault::outcomes::NANOSECONDS_PER_SIMULATION_TICK;

const RELEASE_TICK: u64 = 9;
const THROTTLE_BUDGET: u64 = 3;
const LINK_DELAY: u64 = 4;
const LINK_QUEUE: u32 = 8;
const FIXTURE_MEMORY_BYTES: usize = 16 * 1024 * 1024;

fn controller_with_nodes(nodes: usize) -> SimulationController {
    let mut controller = adapter_test_controller();
    controller.config.vm_config.memory_size = FIXTURE_MEMORY_BYTES;
    controller.vms[0].vm = DeterministicVm::new(VmConfig {
        memory_size: FIXTURE_MEMORY_BYTES,
        ..VmConfig::default()
    })
    .unwrap();
    while controller.vms.len() < nodes {
        let mut slot = adapter_test_controller().vms.pop().unwrap();
        slot.vm = DeterministicVm::new(VmConfig {
            memory_size: FIXTURE_MEMORY_BYTES,
            ..VmConfig::default()
        })
        .unwrap();
        controller.vms.push(slot);
        controller.vm_memory_bases.push(None);
    }
    controller.config.num_vms = nodes;
    controller.network = NetworkFabric::new(nodes, controller.config.seed);
    controller
}

fn directional(disruption: NetworkDisruption) -> Fault {
    Fault::NetworkDirectional {
        node: 0,
        peer: 1,
        direction: NetworkDirection::Outbound,
        disruption,
        delay_ns: if disruption == NetworkDisruption::Stopped {
            0
        } else {
            LINK_DELAY * NANOSECONDS_PER_SIMULATION_TICK
        },
        queue_limit: if disruption == NetworkDisruption::Jammed {
            LINK_QUEUE
        } else {
            0
        },
    }
}

fn serialized_snapshot(controller: &mut SimulationController) -> SimulationSnapshot {
    let snapshot = controller.snapshot_all().unwrap();
    let mut bytes = Vec::new();
    ciborium::ser::into_writer(&snapshot, &mut bytes).unwrap();
    ciborium::de::from_reader(bytes.as_slice()).unwrap()
}

#[test]
fn throttle_snapshot_restores_exact_budget_and_release_and_rejects_drift() {
    // r[verify chaoscontrol.fault_mechanisms.fixtures]
    let mut controller = controller_with_nodes(1);
    controller
        .apply_fault(&Fault::CpuThrottle {
            target: 0,
            budget_units: THROTTLE_BUDGET,
            duration_ticks: RELEASE_TICK,
        })
        .unwrap();
    let snapshot = serialized_snapshot(&mut controller);
    let ledger = snapshot.fault_engine_snapshot.outcomes().clone();
    assert_eq!(ledger.counters.observed, 1);
    controller.tick = RELEASE_TICK;
    assert_eq!(controller.effective_exit_budget(0, 100), 100);
    controller.restore_all(&snapshot).unwrap();
    assert_eq!(controller.effective_exit_budget(0, 100), THROTTLE_BUDGET);
    controller.tick = RELEASE_TICK - 1;
    assert_eq!(controller.effective_exit_budget(0, 100), THROTTLE_BUDGET);
    controller.tick = RELEASE_TICK;
    assert_eq!(controller.effective_exit_budget(0, 100), 100);
    assert!(controller.vms[0].cpu_throttle.is_none());
    assert_eq!(controller.fault_outcomes(), &ledger);

    for corruption in 0..5 {
        let mut forged = snapshot.clone();
        let throttle = forged.cpu_throttle[0].as_mut().unwrap();
        match corruption {
            0 => throttle.budget_units += 1,
            1 => throttle.release_at_tick += 1,
            2 => throttle.budget_units = 0,
            3 => forged.tick = RELEASE_TICK,
            _ => forged.cpu_throttle[0] = None,
        }
        let before = (
            controller.tick,
            controller.vms[0].cpu_throttle,
            controller.fault_outcomes().clone(),
        );
        assert!(matches!(
            controller.restore_all(&forged),
            Err(VmError::Snapshot { .. })
        ));
        assert_eq!(
            (
                controller.tick,
                controller.vms[0].cpu_throttle,
                controller.fault_outcomes().clone()
            ),
            before
        );
    }
}

#[test]
fn directional_modes_snapshot_replay_preserves_packets_and_observation_identity() {
    // r[verify chaoscontrol.fault_mechanisms.directional_network]
    // r[verify chaoscontrol.fault_mechanisms.fixtures]
    for mode in [
        NetworkDisruption::Stopped,
        NetworkDisruption::Slowed,
        NetworkDisruption::Jammed,
    ] {
        let mut controller = controller_with_nodes(2);
        controller.apply_fault(&directional(mode)).unwrap();
        let snapshot = serialized_snapshot(&mut controller);
        let mut tails = Vec::new();
        for _ in 0..2 {
            controller.restore_all(&snapshot).unwrap();
            let outbound = controller.network.send_packet(0, 1, vec![0xAA], 0);
            assert_eq!(outbound, mode != NetworkDisruption::Stopped);
            assert!(controller.network.send_packet(1, 0, vec![0xBB], 0));
            controller
                .commit_pending_network_observations(MAX_FAULT_OUTCOME_EVENTS)
                .unwrap();
            assert_eq!(controller.fault_outcomes().counters.observed, 1);
            tails.push((
                serde_json::to_value(&controller.network).unwrap(),
                controller.fault_outcomes().clone(),
            ));
        }
        assert_eq!(tails[0], tails[1], "{mode:?}");

        for corruption in 0..6 {
            let mut forged = snapshot.clone();
            let fault = &mut forged.network_state.directional[0];
            match corruption {
                0 => fault.direction = NetworkDirection::Inbound,
                1 => fault.delay_ticks += 1,
                2 => fault.queue_limit += 1,
                3 => fault.attempt_id = None,
                4 => fault.attempt_id = Some(FaultAttemptId([0xEE; 32])),
                _ => forged.network_state.directional.clear(),
            }
            let before = serde_json::to_value(&controller.network).unwrap();
            assert!(
                controller.restore_all(&forged).is_err(),
                "{mode:?}/{corruption}"
            );
            assert_eq!(serde_json::to_value(&controller.network).unwrap(), before);
        }
    }
}

#[test]
fn overlapping_links_snapshot_replay_preserves_precedence_and_scoped_heal() {
    // r[verify chaoscontrol.fault_mechanisms.interactions]
    // r[verify chaoscontrol.fault_mechanisms.fixtures]
    let mut controller = controller_with_nodes(3);
    controller
        .apply_fault(&directional(NetworkDisruption::Slowed))
        .unwrap();
    controller
        .apply_fault(&directional(NetworkDisruption::Stopped))
        .unwrap();
    controller
        .apply_fault(&Fault::NetworkDirectional {
            node: 1,
            peer: 2,
            direction: NetworkDirection::Outbound,
            disruption: NetworkDisruption::Jammed,
            delay_ns: LINK_DELAY * NANOSECONDS_PER_SIMULATION_TICK,
            queue_limit: LINK_QUEUE,
        })
        .unwrap();
    assert!(controller.network.send_packet(1, 2, vec![0xCC], 0));
    let snapshot = serialized_snapshot(&mut controller);
    assert_eq!(snapshot.network_state.directional.len(), 3);
    let mut tails = Vec::new();
    for _ in 0..2 {
        controller.restore_all(&snapshot).unwrap();
        assert!(!controller.network.send_packet(0, 1, vec![0xAA], 0));
        controller
            .commit_pending_network_observations(MAX_FAULT_OUTCOME_EVENTS)
            .unwrap();
        controller
            .apply_fault(&Fault::NetworkDirectionalHeal { node: 0, peer: 1 })
            .unwrap();
        assert!(controller.network.send_packet(0, 1, vec![0xBB], 0));
        assert!(controller.network.send_packet(1, 2, vec![0xDD], 0));
        assert_eq!(controller.network.directional.len(), 1);
        controller
            .commit_pending_network_observations(MAX_FAULT_OUTCOME_EVENTS)
            .unwrap();
        tails.push((
            serde_json::to_value(&controller.network).unwrap(),
            controller.fault_outcomes().clone(),
        ));
    }
    assert_eq!(tails[0], tails[1]);
}

#[test]
fn custom_and_pause_snapshot_replay_preserves_active_effects_and_suppression() {
    // r[verify chaoscontrol.fault_mechanisms.custom_effect]
    // r[verify chaoscontrol.fault_mechanisms.pause_control]
    // r[verify chaoscontrol.fault_mechanisms.fixtures]
    let mut controller = controller_with_nodes(1);
    let calls = Arc::new(AtomicUsize::new(0));
    controller.set_custom_effect_adapter(Box::new(RecordingCustomAdapter {
        calls: Arc::clone(&calls),
    }));
    let custom = Fault::CustomEffect {
        effect_id: "test-effect".to_string(),
        version: 1,
        parameters: Default::default(),
    };
    let schedule = FaultScheduleBuilder::new()
        .at_ns(0, custom.clone())
        .at_ns(
            0,
            Fault::CpuThrottle {
                target: 0,
                budget_units: THROTTLE_BUDGET,
                duration_ticks: RELEASE_TICK,
            },
        )
        .at_ns(NANOSECONDS_PER_SIMULATION_TICK, custom)
        .build();
    controller
        .fault_engine
        .begin_counterfactual_run(schedule)
        .unwrap();
    controller.fault_engine.force_setup_complete();
    let attempts = controller.fault_engine.poll_fault_attempts(0).unwrap();
    for attempt in attempts {
        controller.handle_fault_attempt(&attempt).unwrap();
    }
    controller
        .request_pause_control(PauseControlRequest {
            requester: "snapshot-fixture".to_string(),
            start_tick: 0,
            end_tick: RELEASE_TICK - 1,
        })
        .unwrap();
    let snapshot = serialized_snapshot(&mut controller);
    let mut tails = Vec::new();
    for _ in 0..2 {
        controller.restore_all(&snapshot).unwrap();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "restore must not invoke an applied custom effect"
        );
        assert_eq!(controller.effective_exit_budget(0, 100), THROTTLE_BUDGET);
        let controls_before = controller.fault_outcomes().control_events.len();
        assert!(controller
            .fault_engine
            .poll_fault_attempts(NANOSECONDS_PER_SIMULATION_TICK)
            .unwrap()
            .is_empty());
        assert!(controller.fault_outcomes().control_events.len() > controls_before);
        controller.tick = RELEASE_TICK - 1;
        assert_eq!(controller.effective_exit_budget(0, 100), THROTTLE_BUDGET);
        controller.tick = RELEASE_TICK;
        assert_eq!(controller.effective_exit_budget(0, 100), 100);
        tails.push(controller.fault_engine.snapshot().outcomes().clone());
    }
    assert_eq!(tails[0], tails[1]);

    let mut bytes = Vec::new();
    ciborium::ser::into_writer(&snapshot.fault_engine_snapshot, &mut bytes).unwrap();
    let mut value: ciborium::value::Value = ciborium::de::from_reader(bytes.as_slice()).unwrap();
    let ciborium::value::Value::Map(fields) = &mut value else {
        panic!("engine snapshot map")
    };
    let pause_control = fields
        .iter_mut()
        .find(|(key, _)| *key == ciborium::value::Value::Text("pause_control".into()))
        .map(|(_, value)| value)
        .unwrap();
    let ciborium::value::Value::Array(windows) = pause_control else {
        panic!("pause windows")
    };
    let ciborium::value::Value::Map(window) = &mut windows[0] else {
        panic!("pause window map")
    };
    *window
        .iter_mut()
        .find(|(key, _)| *key == ciborium::value::Value::Text("end_tick".into()))
        .map(|(_, value)| value)
        .unwrap() = ciborium::value::Value::Integer(0.into());
    bytes.clear();
    ciborium::ser::into_writer(&value, &mut bytes).unwrap();
    let mut malformed = snapshot;
    malformed.fault_engine_snapshot = ciborium::de::from_reader(bytes.as_slice()).unwrap();
    let before = controller.fault_outcomes().clone();
    assert!(controller.restore_all(&malformed).is_err());
    assert_eq!(controller.fault_outcomes(), &before);
}

#[test]
fn selected_before_pause_snapshot_replays_application_suppression_without_effects() {
    // r[verify chaoscontrol.fault_mechanisms.pause_control]
    // r[verify chaoscontrol.fault_mechanisms.fixtures]
    let mut controller = controller_with_nodes(1);
    let schedule = FaultScheduleBuilder::new()
        .at_ns(
            0,
            Fault::CpuThrottle {
                target: 0,
                budget_units: THROTTLE_BUDGET,
                duration_ticks: RELEASE_TICK,
            },
        )
        .build();
    controller
        .fault_engine
        .begin_counterfactual_run(schedule)
        .unwrap();
    controller.fault_engine.force_setup_complete();
    let attempt = controller
        .fault_engine
        .poll_fault_attempts(0)
        .unwrap()
        .pop()
        .unwrap();
    controller
        .request_pause_control(PauseControlRequest {
            requester: "selected-application-fixture".into(),
            start_tick: 0,
            end_tick: RELEASE_TICK,
        })
        .unwrap();
    let snapshot = serialized_snapshot(&mut controller);
    let mut tails = Vec::new();
    for _ in 0..2 {
        controller.restore_all(&snapshot).unwrap();
        controller.handle_fault_attempt(&attempt).unwrap();
        assert!(controller.vms[0].cpu_throttle.is_none());
        let ledger = controller.fault_outcomes();
        assert_eq!(ledger.counters.selected, 1);
        assert_eq!(ledger.counters.rejected, 1);
        assert_eq!(ledger.counters.applied, 0);
        assert_eq!(ledger.counters.observed, 0);
        assert!(matches!(
            ledger.control_events.last().unwrap().kind,
            chaoscontrol_fault::evidence::FaultControlKind::ApplicationSuppressed { .. }
        ));
        tails.push(ledger.clone());
    }
    assert_eq!(tails[0], tails[1]);
}

#[test]
fn custom_snapshot_continuation_rejects_missing_adapter_without_invocation() {
    // r[verify chaoscontrol.fault_mechanisms.custom_effect]
    // r[verify chaoscontrol.fault_mechanisms.fixtures]
    let mut controller = controller_with_nodes(1);
    let calls = Arc::new(AtomicUsize::new(0));
    controller.set_custom_effect_adapter(Box::new(RecordingCustomAdapter {
        calls: Arc::clone(&calls),
    }));
    let schedule = FaultScheduleBuilder::new()
        .at_ns(
            0,
            Fault::CustomEffect {
                effect_id: "test-effect".into(),
                version: 1,
                parameters: Default::default(),
            },
        )
        .build();
    controller
        .fault_engine
        .begin_counterfactual_run(schedule)
        .unwrap();
    controller.fault_engine.force_setup_complete();
    let attempt = controller
        .fault_engine
        .poll_fault_attempts(0)
        .unwrap()
        .pop()
        .unwrap();
    let snapshot = serialized_snapshot(&mut controller);
    controller.handle_fault_attempt(&attempt).unwrap();
    let observed = controller.fault_outcomes().clone();
    assert_eq!(observed.counters.observed, 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    controller.restore_all(&snapshot).unwrap();
    controller.custom_effect_adapter = None;
    controller.handle_fault_attempt(&attempt).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(controller.fault_outcomes().counters.rejected, 1);
    assert_eq!(controller.fault_outcomes().counters.applied, 0);
    assert_eq!(controller.fault_outcomes().counters.observed, 0);
    assert_ne!(controller.fault_outcomes().events, observed.events);
}
