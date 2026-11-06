//! Exact replay contract fixtures use the existing trace runner. Actual device
//! and adapter execution is covered by the VMM snapshot fixtures, not this runner.
use super::*;
use chaoscontrol_fault::engine::{EngineConfig, FaultEngine};
use chaoscontrol_fault::mechanisms::{CustomEffectIdentity, NetworkDirection, NetworkDisruption};
use chaoscontrol_fault::outcomes::{
    plan_fault_application, FaultApplicationPolicy, FaultObservation, FaultObservationEffect,
    FaultObservationSubsystem, FaultPlanningFacts, FaultVmStatus, VmFaultFacts,
};
use chaoscontrol_fault::schedule::{FaultScheduleBuilder, ScheduledFault};

pub(super) const MECHANISM_TRACE_SEED: u64 = 5_204;
const WINDOW_TICKS: u64 = 9;
const DELAY_TICKS: u64 = 4;
const QUEUE_PACKETS: u32 = 8;
const FIXTURE_MEMORY_BYTES: u64 = 8_192;
const FIXTURE_TSC_KHZ: u32 = 3_000_000;

fn planning_facts() -> FaultPlanningFacts {
    let vm = VmFaultFacts {
        status: FaultVmStatus::Running,
        vcpu_count: 1,
        memory_size_bytes: FIXTURE_MEMORY_BYTES,
        block_device_size_bytes: None,
        has_initial_snapshot: false,
        supports_irq: false,
        supports_nmi: false,
        supports_clock_freeze: false,
        supports_clock_jitter: false,
        supports_cpu_stall: false,
        supports_cpu_throttle: true,
        supports_thread_pause: false,
        supports_memory_pressure: false,
        virtual_tsc: 0,
        tsc_khz: FIXTURE_TSC_KHZ,
    };
    FaultPlanningFacts {
        current_tick: 1,
        network_supported: true,
        custom_effect: Some(CustomEffectIdentity {
            effect_id: "test-effect".into(),
            version: 1,
        }),
        vms: vec![vm.clone(), vm],
    }
}

pub(super) fn recording_for_schedule(schedule: FaultSchedule) -> Recording {
    let mut recording = test_recording();
    recording.seed = MECHANISM_TRACE_SEED;
    recording.schedule = schedule.clone();
    recording.fault_run_id = fault_run_id(
        recording.seed,
        recording.fault_run_sequence,
        schedule.identity(),
    );
    recording.checkpoints = CheckpointStore::new();
    recording.total_ticks = 1;
    let mut engine = FaultEngine::new(EngineConfig {
        seed: recording.seed,
        schedule: Some(schedule),
        ..EngineConfig::default()
    });
    engine.start_fresh_run_at(recording.schedule.clone(), recording.fault_run_sequence);
    engine.force_setup_complete();
    let attempts = engine
        .poll_fault_attempts(NANOSECONDS_PER_SIMULATION_TICK)
        .unwrap();
    for (index, attempt) in attempts.iter().enumerate() {
        let plan = plan_fault_application(
            attempt,
            &planning_facts(),
            &FaultApplicationPolicy::default(),
        )
        .unwrap();
        engine
            .record_fault_stage(
                attempt.id,
                FaultStageKind::Applicable {
                    effect: plan.effect.clone(),
                },
            )
            .unwrap();
        engine
            .record_fault_stage(
                attempt.id,
                FaultStageKind::Applied {
                    effect: plan.effect.clone(),
                },
            )
            .unwrap();
        let observation = match plan.effect {
            FaultPlanEffect::NetworkDirectional { disruption, .. } => Some((
                FaultObservationSubsystem::Network,
                match disruption {
                    NetworkDisruption::Stopped => {
                        FaultObservationEffect::PacketDroppedByDirectional
                    }
                    NetworkDisruption::Slowed => FaultObservationEffect::PacketDelayedByDirectional,
                    NetworkDisruption::Jammed => FaultObservationEffect::PacketJammedByDirectional,
                },
            )),
            FaultPlanEffect::CpuThrottle { .. } => Some((
                FaultObservationSubsystem::Cpu,
                FaultObservationEffect::CpuThrottleActivated,
            )),
            FaultPlanEffect::CustomEffect { .. } => Some((
                FaultObservationSubsystem::Process,
                FaultObservationEffect::CustomEffectApplied,
            )),
            FaultPlanEffect::NetworkDirectionalHeal { .. } => None,
            _ => panic!("fixture contains an unexpected mechanism"),
        };
        if let Some((subsystem, effect)) = observation {
            engine
                .record_fault_observations(&[FaultObservation::new(
                    attempt.id,
                    subsystem,
                    index as u64,
                    effect,
                )])
                .unwrap();
        }
        recording.events.push(RecordedEvent::FaultFired {
            tick: 1,
            fault: format!("{:?}", attempt.fault),
        });
    }
    recording.fault_outcome_ledger = engine.fault_outcomes().clone();
    recording.fault_stage_events = recording.fault_outcome_ledger.events.clone();
    recording.fault_round_deltas = vec![FaultRoundTraceDelta {
        tick: 1,
        event_start: 0,
        event_end: recording.fault_stage_events.len() as u64,
    }];
    recording
}

fn faults() -> Vec<Fault> {
    let mut faults = [
        NetworkDisruption::Stopped,
        NetworkDisruption::Slowed,
        NetworkDisruption::Jammed,
    ]
    .into_iter()
    .map(|disruption| Fault::NetworkDirectional {
        node: 0,
        peer: 1,
        direction: NetworkDirection::Outbound,
        disruption,
        delay_ns: if disruption == NetworkDisruption::Stopped {
            0
        } else {
            DELAY_TICKS * NANOSECONDS_PER_SIMULATION_TICK
        },
        queue_limit: if disruption == NetworkDisruption::Jammed {
            QUEUE_PACKETS
        } else {
            0
        },
    })
    .collect::<Vec<_>>();
    faults.extend([
        Fault::NetworkDirectionalHeal { node: 0, peer: 1 },
        Fault::CpuThrottle {
            target: 0,
            budget_units: 3,
            duration_ticks: WINDOW_TICKS,
        },
        Fault::CustomEffect {
            effect_id: "test-effect".into(),
            version: 1,
            parameters: Default::default(),
        },
    ]);
    faults
}

#[test]
fn every_new_mechanism_saved_trace_replays_exactly_and_rejects_plan_drift() {
    // r[verify chaoscontrol.fault_mechanisms.fixtures]
    for fault in faults() {
        let schedule = FaultScheduleBuilder::new()
            .at_ns(NANOSECONDS_PER_SIMULATION_TICK, fault.clone())
            .build();
        let recording = recording_for_schedule(schedule);
        let directory = tempfile::TempDir::new().unwrap();
        let path = directory.path().join("mechanism-recording.json");
        crate::serialize::save_recording(&recording, &path).unwrap();
        let loaded = crate::serialize::load_recording(&path).unwrap();
        let replay: ReplayEngine<MockRunner> = ReplayEngine::new(loaded.clone());
        assert_eq!(
            replay.replay_from(None, 1).unwrap().fault_outcomes,
            loaded.fault_stage_events,
            "{fault:?}"
        );
        let mut drifted_runner = loaded.clone();
        drifted_runner.config.quantum = CORRUPT_FAULT_TRACE_QUANTUM;
        let divergent_replay: ReplayEngine<MockRunner> = ReplayEngine::new(drifted_runner);
        assert!(
            matches!(
                divergent_replay.replay_from(None, 1),
                Err(ReplayError::InvalidState { .. })
            ),
            "{fault:?}"
        );
        let mut divergent = loaded.fault_stage_events.clone();
        let effect = divergent
            .iter_mut()
            .find_map(|event| match &mut event.kind {
                FaultStageKind::Applicable { effect } => Some(effect),
                _ => None,
            })
            .unwrap();
        match effect {
            FaultPlanEffect::NetworkDirectional { direction, .. } => {
                *direction = NetworkDirection::Inbound
            }
            FaultPlanEffect::NetworkDirectionalHeal { peer, .. } => *peer = 0,
            FaultPlanEffect::CpuThrottle { budget_units, .. } => *budget_units += 1,
            FaultPlanEffect::CustomEffect { effect } => effect.version += 1,
            _ => unreachable!(),
        }
        assert!(
            matches!(
                validate_replayed_fault_round(&loaded, 1, &divergent),
                Err(ReplayError::InvalidState { .. })
            ),
            "{fault:?}"
        );
        assert!(
            matches!(
                validate_replayed_fault_interval(&loaded, 0, 1, &divergent),
                Err(ReplayError::InvalidState { .. })
            ),
            "{fault:?}"
        );
    }
}

#[test]
fn overlapping_mechanism_windows_replay_with_exact_order_and_checkpoint_prefix() {
    // r[verify chaoscontrol.fault_mechanisms.interactions]
    // r[verify chaoscontrol.fault_mechanisms.fixtures]
    let mut schedule = FaultSchedule::new();
    for fault in faults()
        .into_iter()
        .filter(|fault| !matches!(fault, Fault::NetworkDirectionalHeal { .. }))
    {
        schedule.add(ScheduledFault::new(NANOSECONDS_PER_SIMULATION_TICK, fault));
    }
    let mut recording = recording_for_schedule(schedule);
    let mut engine = FaultEngine::new(EngineConfig {
        seed: recording.seed,
        ..EngineConfig::default()
    });
    engine.start_fresh_run_at(recording.schedule.clone(), recording.fault_run_sequence);
    engine.force_setup_complete();
    engine
        .poll_fault_attempts(NANOSECONDS_PER_SIMULATION_TICK)
        .unwrap();
    let mut snapshot = MockRunner::create(
        &recording.config,
        recording.schedule.clone(),
        recording.seed,
        recording.fault_run_sequence,
    )
    .unwrap()
    .snapshot_all()
    .unwrap();
    snapshot.tick = 1;
    snapshot.fault_engine_snapshot = engine.snapshot();
    snapshot
        .fault_engine_snapshot
        .replace_outcomes_for_validation_test(recording.fault_outcome_ledger.clone());
    recording.checkpoints.push(Checkpoint {
        id: 0,
        tick: 1,
        snapshot: Some(snapshot),
        serial_output: vec![],
        events_since_last: vec![],
    });
    assert!(
        validated_checkpoint_snapshot(&recording, recording.checkpoints.get(0).unwrap()).is_ok()
    );
    let replay: ReplayEngine<MockRunner> = ReplayEngine::new(recording.clone());
    assert_eq!(
        replay.replay_from(None, 1).unwrap().fault_outcomes,
        recording.fault_stage_events
    );
    assert!(replay
        .replay_from(Some(0), 0)
        .unwrap()
        .fault_outcomes
        .is_empty());
    let mut reordered = recording.fault_stage_events.clone();
    reordered.swap(0, 1);
    assert!(validate_replayed_fault_round(&recording, 1, &reordered).is_err());
    let mut drifted = recording.checkpoints.get(0).unwrap().clone();
    let empty = MockRunner::create(
        &recording.config,
        recording.schedule.clone(),
        recording.seed,
        recording.fault_run_sequence,
    )
    .unwrap()
    .snapshot_all()
    .unwrap();
    drifted.snapshot.as_mut().unwrap().fault_engine_snapshot = empty.fault_engine_snapshot;
    assert!(matches!(
        validated_checkpoint_snapshot(&recording, &drifted),
        Err(ReplayError::InvalidState { .. })
    ));
}

pub(super) const PAUSE_TRACE_SEED: u64 = 5_205;

fn pause_recording() -> Recording {
    let mut recording = test_recording();
    recording.seed = PAUSE_TRACE_SEED;
    recording.schedule = FaultScheduleBuilder::new()
        .at_ns(
            NANOSECONDS_PER_SIMULATION_TICK,
            Fault::CpuThrottle {
                target: 0,
                budget_units: 3,
                duration_ticks: WINDOW_TICKS,
            },
        )
        .build();
    recording.fault_run_id = fault_run_id(
        recording.seed,
        recording.fault_run_sequence,
        recording.schedule.identity(),
    );
    recording.checkpoints = CheckpointStore::new();
    recording.events.clear();
    recording.total_ticks = 2;
    let mut runner = MockRunner::create(
        &recording.config,
        recording.schedule.clone(),
        recording.seed,
        recording.fault_run_sequence,
    )
    .unwrap();
    let request = chaoscontrol_fault::mechanisms::PauseControlRequest {
        requester: "replay-fixture".into(),
        start_tick: 0,
        end_tick: 2,
    };
    runner.replay_pause_request(request.clone()).unwrap();
    runner.replay_pause_request(request).unwrap(); // Typed duplicate rejection is an input fact too.
    for tick in 0..=1 {
        recording.checkpoints.push(Checkpoint {
            id: tick,
            tick,
            snapshot: Some(runner.snapshot_all().unwrap()),
            serial_output: vec![],
            events_since_last: vec![],
        });
        let round = runner.step_round().unwrap();
        if !round.fault_outcomes.is_empty() {
            recording.fault_round_deltas.push(FaultRoundTraceDelta {
                tick: round.tick,
                event_start: 0,
                event_end: round.fault_outcomes.len() as u64,
            });
            recording.fault_stage_events.extend(round.fault_outcomes);
        }
        for fault in round.faults_fired {
            recording.events.push(RecordedEvent::FaultFired {
                tick: round.tick,
                fault: format!("{fault:?}"),
            });
        }
    }
    recording.fault_outcome_ledger = runner.control_engine.fault_outcomes().clone();
    recording
}

#[test]
fn pause_replay_dispatches_recorded_admission_and_preserves_mid_window_suppression() {
    // r[verify chaoscontrol.fault_mechanisms.pause_control]
    // r[verify chaoscontrol.fault_mechanisms.fixtures]
    let recording = pause_recording();
    let directory = tempfile::TempDir::new().unwrap();
    let path = directory.path().join("pause-recording.json");
    crate::serialize::save_recording(&recording, &path).unwrap();
    let mut loaded = crate::serialize::load_recording(&path).unwrap();
    assert!(
        loaded.checkpoints.is_empty(),
        "recording JSON is metadata-only"
    );
    let from_start: ReplayEngine<MockRunner> = ReplayEngine::new(loaded.clone());
    assert!(from_start.replay_from(None, 2).is_ok());
    // Checkpoint artifacts are retained separately from metadata-only recording
    // JSON. Persist and reload both their identity and serialized snapshot state.
    for checkpoint in recording.checkpoints.all() {
        let artifact_path = directory
            .path()
            .join(format!("checkpoint-{}.cbor", checkpoint.id));
        let artifact = (
            checkpoint.id,
            checkpoint.tick,
            checkpoint.snapshot.as_ref().unwrap(),
        );
        let mut bytes = Vec::new();
        ciborium::ser::into_writer(&artifact, &mut bytes).unwrap();
        std::fs::write(&artifact_path, bytes).unwrap();
        let (id, tick, snapshot): (u64, u64, SimulationSnapshot) =
            ciborium::de::from_reader(std::fs::File::open(&artifact_path).unwrap()).unwrap();
        loaded.checkpoints.push(Checkpoint {
            id,
            tick,
            snapshot: Some(snapshot),
            serial_output: vec![],
            events_since_last: vec![],
        });
    }
    let expected = &loaded.fault_outcome_ledger.control_events;
    assert_eq!(expected.len(), 3); // Admitted request, duplicate rejection, suppressed opportunity.
    for checkpoint in [None, Some(0), Some(1)] {
        let ticks = if checkpoint == Some(1) { 1 } else { 2 };
        let replay: ReplayEngine<MockRunner> = ReplayEngine::new(loaded.clone());
        let result = replay.replay_from(checkpoint, ticks).unwrap();
        assert_eq!(
            &result
                .final_snapshot
                .unwrap()
                .fault_engine_snapshot
                .outcomes()
                .control_events,
            expected
        );
        assert_eq!(result.fault_outcomes, loaded.fault_stage_events);
    }
    let mut debugger: crate::debugger::Debugger<MockRunner> =
        crate::debugger::Debugger::new(loaded.clone());
    assert_eq!(debugger.goto(1).unwrap().tick, 1);
    assert_eq!(debugger.step_forward(1).unwrap().tick, 2);
    assert_eq!(debugger.rewind(1).unwrap().tick, 1);
    let mut changed_admission = loaded;
    changed_admission.config.quantum = CORRUPT_FAULT_TRACE_QUANTUM;
    let divergent: ReplayEngine<MockRunner> = ReplayEngine::new(changed_admission);
    assert!(matches!(
        divergent.replay_from(None, 2),
        Err(ReplayError::InvalidState { .. })
    ));
}

#[test]
fn pause_exact_replay_rejects_missing_changed_and_reordered_control_facts() {
    // r[verify chaoscontrol.fault_mechanisms.fixtures]
    let recording = pause_recording();
    let expected = &recording.fault_outcome_ledger.control_events;
    let stage_count = recording.fault_stage_events.len() as u64;
    assert!(validate_replayed_fault_controls(&recording, 2, stage_count, expected).is_ok());
    let mut missing = expected.clone();
    missing.pop();
    assert!(validate_replayed_fault_controls(&recording, 2, stage_count, &missing).is_err());
    let mut changed = expected.clone();
    changed.last_mut().unwrap().tick += 1;
    assert!(validate_replayed_fault_controls(&recording, 2, stage_count, &changed).is_err());
    let mut reordered = expected.clone();
    reordered.swap(0, 1);
    assert!(validate_replayed_fault_controls(&recording, 2, stage_count, &reordered).is_err());

    // Stage prefixes are identical here: only suppression evidence distinguishes
    // the mid-window checkpoint from the initial checkpoint.
    let mut drifted = recording.checkpoints.get(1).unwrap().clone();
    drifted.snapshot.as_mut().unwrap().fault_engine_snapshot = recording
        .checkpoints
        .get(0)
        .unwrap()
        .snapshot
        .as_ref()
        .unwrap()
        .fault_engine_snapshot
        .clone();
    assert!(matches!(
        validated_checkpoint_snapshot(&recording, &drifted),
        Err(ReplayError::InvalidState { .. })
    ));
}

#[test]
fn exact_replay_fails_closed_when_rejected_pause_request_raw_identity_is_absent() {
    // r[verify chaoscontrol.fault_mechanisms.fixtures]
    let mut recording = test_recording();
    recording.seed = PAUSE_TRACE_SEED;
    recording.checkpoints = CheckpointStore::new();
    recording.total_ticks = 0;
    recording.fault_run_id = fault_run_id(
        recording.seed,
        recording.fault_run_sequence,
        recording.schedule.identity(),
    );
    let mut runner = MockRunner::create(
        &recording.config,
        recording.schedule.clone(),
        recording.seed,
        recording.fault_run_sequence,
    )
    .unwrap();
    runner
        .replay_pause_request(chaoscontrol_fault::mechanisms::PauseControlRequest {
            requester: "x".repeat(chaoscontrol_fault::mechanisms::MAX_CUSTOM_STRING_BYTES + 1),
            start_tick: 0,
            end_tick: 1,
        })
        .unwrap();
    recording.fault_outcome_ledger = runner.control_engine.fault_outcomes().clone();
    assert!(validate_recording(&recording).is_ok());
    let replay: ReplayEngine<MockRunner> = ReplayEngine::new(recording);
    assert!(
        matches!(replay.replay_from(None, 0), Err(ReplayError::InvalidState { message }) if message.contains("no retained raw identity"))
    );
}
