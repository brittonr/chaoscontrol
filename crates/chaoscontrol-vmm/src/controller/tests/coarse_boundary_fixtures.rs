use super::*;
use crate::scheduler::core::{BoundaryProfileId, ProgressBudget, ProgressMode};
use crate::vm::diagnostic_fixtures::{
    coarse_smp_vm, stage_coarse_irq, COARSE_MEMORY_BYTES, COARSE_TIMEOUT_WATCHDOG_US,
};

const VM_COUNT: usize = 2;
const MAIN: &[u8] = &[0x40, 0xeb, 0xfd]; // inc ax; jmp main
const NO_EXIT_HANDLER: &[u8] = &[0xeb, 0xfe]; // IRQ entry suppresses TF; jmp $

fn coarse_controller() -> SimulationController {
    let mut config = SimulationConfig {
        num_vms: VM_COUNT,
        vm_config: VmConfig {
            memory_size: COARSE_MEMORY_BYTES,
            num_vcpus: 2,
            smp_progress_mode: ProgressMode::KvmDebugBoundary,
            smp_progress_budget: ProgressBudget::KvmBoundaries {
                profile_id: BoundaryProfileId::KvmDebugExitBoundaryV1,
                count: 1,
            },
            smp_entry_watchdog_microseconds: COARSE_TIMEOUT_WATCHDOG_US,
            ..VmConfig::default()
        },
        quantum: 1,
        kernel_path: dummy_kernel_path(),
        ..SimulationConfig::default()
    };
    config
        .vm_config
        .apply_kvm_debug_boundary_profile(BoundaryProfileId::KvmDebugExitBoundaryV1)
        .unwrap();
    let mut vms = Vec::with_capacity(VM_COUNT);
    for _ in 0..VM_COUNT {
        vms.push(VmSlot {
            vm: coarse_smp_vm(MAIN, NO_EXIT_HANDLER, 1, COARSE_TIMEOUT_WATCHDOG_US),
            status: VmStatus::Running,
            inbox: VecDeque::new(),
            disk_faults: DiskFaultFlags::default(),
            tsc_skew: 0,
            memory_limit_bytes: None,
            memory_limit_release_at_tick: None,
            initial_snapshot: None,
            vcpu_stall_until: std::collections::BTreeMap::new(),
            cpu_throttle: None,
            clock_freeze: None,
            clock_jitter_bound: 0,
            process_fault_attempt: None,
        });
    }
    let mut fault_engine = FaultEngine::new(fault_engine_config(&config));
    fault_engine.begin_run();
    SimulationController {
        vms,
        fault_engine,
        network: NetworkFabric::new(VM_COUNT, config.seed),
        tick: 0,
        quantum: config.quantum,
        config,
        guest_artifact_ids: Vec::new(),
        vm_memory_bases: vec![None; VM_COUNT],
        fault_application_policy: FaultApplicationPolicy::default(),
        custom_effect_adapter: None,
        fault_operation_sequence: 0,
        pending_process_observations: VecDeque::new(),
        assertion_checkpoints: Vec::new(),
        round_poison: None,
    }
}

#[test]
#[ignore = "real KVM partial multi-VM coarse timeout; requires finite timeout --kill-after supervisor"]
// r[verify chaoscontrol.kvm_debug_boundary.recovery]
// r[verify chaoscontrol.kvm_debug_boundary.evidence]
fn coarse_partial_round_timeout_permanently_blocks_results_snapshots_restore_and_trace() {
    let mut controller = coarse_controller();
    stage_coarse_irq(&mut controller.vms[1].vm);
    let checkpoint = controller.snapshot_all().unwrap();
    let first_before = controller.vms[0].vm.scheduler().state_id();
    let second_before = controller.vms[1].vm.scheduler().state_id();
    let second_cpu_before = controller.vms[1].vm.read_vcpu_registers(1).unwrap();
    let second_clock_before = controller.vms[1].vm.virtual_tsc();
    let start = std::time::Instant::now();
    assert!(matches!(
        controller.step_round(),
        Err(VmError::HostWatchdogTimeout { vcpu: 0 })
    ));
    assert!(start.elapsed() < std::time::Duration::from_secs(5));
    assert!(controller.round_poison().is_some());
    assert_eq!(controller.tick(), 0);
    assert_ne!(
        controller.vms[0].vm.scheduler().state_id(),
        first_before,
        "the first VM really executed before the second timed out"
    );
    assert_eq!(
        controller.vms[0].vm.scheduler().state().progress_counts(),
        &[1, 0]
    );
    let stuck = &controller.vms[1].vm;
    assert!(stuck.schedule_execution_poison().is_some());
    assert_eq!(stuck.scheduler().state_id(), second_before);
    assert_eq!(stuck.scheduler().active(), 0);
    assert_eq!(stuck.scheduler().state().progress_counts(), &[0, 0]);
    assert!(stuck.scheduler().state().runnable_vcpus[1]);
    assert_eq!(stuck.read_vcpu_registers(1).unwrap(), second_cpu_before);
    assert_eq!(stuck.virtual_tsc(), second_clock_before);
    assert!(matches!(
        controller.vms[1].vm.take_schedule_trace(),
        Err(VmError::ScheduleExecutionPoisoned { .. })
    ));
    assert!(matches!(
        controller.step_round(),
        Err(VmError::ControllerRoundPoisoned { .. })
    ));
    assert!(matches!(
        controller.run(0),
        Err(VmError::ControllerRoundPoisoned { .. })
    ));
    assert!(matches!(
        controller.snapshot_all(),
        Err(VmError::ControllerRoundPoisoned { .. })
    ));
    assert!(matches!(
        controller.snapshot_all_incremental(),
        Err(VmError::ControllerRoundPoisoned { .. })
    ));
    assert!(matches!(
        controller.restore_all(&checkpoint),
        Err(VmError::ControllerRoundPoisoned { .. })
    ));
    assert!(matches!(
        controller.restore_all_incremental(&checkpoint),
        Err(VmError::ControllerRoundPoisoned { .. })
    ));
    assert!(matches!(
        controller.settle_exact_observation(),
        Err(VmError::ControllerRoundPoisoned { .. })
    ));
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || controller.take_assertion_checkpoints()
    ))
    .is_err());
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| controller.report())).is_err()
    );
    let mut fresh = coarse_controller();
    fresh
        .restore_all(&checkpoint)
        .expect("only a fresh admitted controller may restore the checkpoint");
}

#[test]
#[ignore = "real KVM coarse VM poison outside a round; requires finite outer supervisor"]
// r[verify chaoscontrol.kvm_debug_boundary.recovery]
fn externally_poisoned_vm_blocks_controller_snapshot_before_round_retry() {
    let mut controller = coarse_controller();
    stage_coarse_irq(&mut controller.vms[1].vm);
    assert!(matches!(
        controller.vms[1].vm.run_bounded(1),
        Err(VmError::HostWatchdogTimeout { .. })
    ));
    assert!(controller.round_poison().is_none());
    assert!(matches!(
        controller.snapshot_all(),
        Err(VmError::ControllerRoundPoisoned { .. })
    ));
    assert!(controller.round_poison().is_some());
    assert!(matches!(
        controller.step_round(),
        Err(VmError::ControllerRoundPoisoned { .. })
    ));
}

#[test]
// r[verify chaoscontrol.kvm_debug_boundary.persistence]
fn canonical_injection_trace_parser_rejects_unknown_claims_and_oversized_headers() {
    use crate::snapshot::{InjectionKind, InjectionRecord, InjectionSource};
    let trace = VmInjectionTrace {
        vm_id: 0,
        records: vec![InjectionRecord {
            target_vcpu: None,
            kind: InjectionKind::Irq {
                irq: 4,
                level: true,
            },
            source: InjectionSource::Shell,
            requested_state_id: [1; 32],
            accepted: true,
        }],
    };
    let value = serde_json::to_value(&trace).unwrap();
    assert_eq!(
        serde_json::from_value::<VmInjectionTrace>(value.clone()).unwrap(),
        trace
    );
    let mut forged_wrapper = value.clone();
    forged_wrapper["delivered_at_instruction"] = 1.into();
    assert!(serde_json::from_value::<VmInjectionTrace>(forged_wrapper).is_err());
    let mut forged_record = value;
    forged_record["records"][0]["delivered_at_instruction"] = 1.into();
    assert!(serde_json::from_value::<VmInjectionTrace>(forged_record).is_err());
    // Definite-length CBOR map {vm_id:0, records:[...]} with an oversized
    // array header and no body: reject its declared count before allocation or
    // attempting to parse a missing record, not merely at end-of-input.
    let mut oversized = vec![
        0xa2, 0x65, b'v', b'm', b'_', b'i', b'd', 0x00, 0x67, b'r', b'e', b'c', b'o', b'r', b'd',
        b's', 0x9a,
    ];
    let count = u32::try_from(crate::scheduler::core::DEFAULT_SCHEDULE_JOURNAL_LIMIT + 1).unwrap();
    oversized.extend_from_slice(&count.to_be_bytes());
    let error = ciborium::de::from_reader::<VmInjectionTrace, _>(&oversized[..]).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("injection trace record bound exceeded"),
        "{error}"
    );
}

#[test]
// r[verify chaoscontrol.kvm_debug_boundary.persistence]
fn simulation_snapshot_requires_explicit_pending_surface_fields() {
    let snapshot = SimulationSnapshot {
        tick: 0,
        vm_snapshots: Vec::new(),
        network_state: NetworkFabric::new(0, 42),
        fault_engine_snapshot: FaultEngine::new(EngineConfig::default()).snapshot(),
        vcpu_stall_until: Vec::new(),
        cpu_throttle: Vec::new(),
        clock_freeze: Vec::new(),
        clock_jitter_bound: Vec::new(),
        memory_pressure: Vec::new(),
        process_fault_attempt: Vec::new(),
        fault_operation_sequence: 0,
        pending_process_observations: VecDeque::new(),
    };
    let valid = serde_json::to_value(&snapshot).unwrap();
    serde_json::from_value::<SimulationSnapshot>(valid.clone()).unwrap();
    for field in ["cpu_throttle", "memory_pressure"] {
        let mut omitted = valid.clone();
        omitted.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<SimulationSnapshot>(omitted).is_err(),
            "{field} must not be defaulted from omitted persisted state"
        );
        let mut null = valid.clone();
        null[field] = serde_json::Value::Null;
        assert!(
            serde_json::from_value::<SimulationSnapshot>(null).is_err(),
            "{field} must not be defaulted from null persisted state"
        );
    }
}

#[test]
#[ignore = "real KVM coarse NMI-capability selected fault rejection; no guest entry"]
// r[verify chaoscontrol.kvm_debug_boundary.persistence]
fn selected_nmi_records_typed_unsupported_without_guest_entry_or_injection() {
    use chaoscontrol_fault::outcomes::FaultRejectionReason;
    let mut controller = coarse_controller();
    let before = controller.snapshot_all().unwrap();
    let states: Vec<_> = controller
        .vms
        .iter()
        .map(|slot| slot.vm.scheduler().state_id())
        .collect();
    let schedule = FaultScheduleBuilder::new()
        .at_ns(0, Fault::InjectNmi { target: 0, vcpu: 0 })
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
        .remove(0);
    controller.handle_fault_attempt(&attempt).unwrap();
    let ledger = controller.fault_outcomes();
    assert_eq!(
        ledger.attempts[&attempt.id].stage,
        FaultAuthoritativeStage::Rejected
    );
    assert!(ledger
        .events
        .iter()
        .any(|event| event.attempt_id == attempt.id
            && matches!(
                event.kind,
                FaultStageKind::Rejected {
                    reason: FaultRejectionReason::UnsupportedCapability {
                        variant: FaultVariant::InjectNmi,
                    }
                }
            )));
    assert_eq!(ledger.counters.applied, 0);
    assert_eq!(ledger.counters.observed, 0);
    assert!(
        controller.round_poison().is_none(),
        "no-entry unsupported capability is not partial execution"
    );
    for (index, slot) in controller.vms.iter_mut().enumerate() {
        assert_eq!(slot.vm.exit_count(), 0);
        assert_eq!(slot.vm.scheduler().state_id(), states[index]);
        assert!(slot.vm.schedule_execution_poison().is_none());
        assert!(slot.vm.take_injection_trace().unwrap().is_empty());
        let after = slot.vm.snapshot().unwrap();
        for (vcpu, saved) in after
            .vcpu_snapshots
            .iter()
            .zip(&before.vm_snapshots[index].0.vcpu_snapshots)
        {
            assert_eq!(vcpu.regs, saved.regs);
            assert_eq!(vcpu.events, saved.events);
        }
    }
}
