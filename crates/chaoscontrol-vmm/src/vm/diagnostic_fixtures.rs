//! Partial KVM-control fixtures, never production admission or exactness proof.
//!
//! The production constructor always rejects strict SMP on portable KVM. These
//! fixtures explicitly add diagnostic vCPUs and model state to an admitted
//! single-vCPU VM so low-level poison, wake, CPUID and bounded-control regressions
//! remain available without fabricating an event-entry exact capability.

use super::*;

pub(crate) fn partial_smp_vm(config: VmConfig) -> DeterministicVm {
    assert!(config.num_vcpus > 1);
    assert_eq!(config.smp_progress_mode, ProgressMode::ExactSingleStep);
    let num_vcpus = config.num_vcpus;
    let effective_cpu = config.effective_cpu_config();
    let mut single_config = config.clone();
    single_config.num_vcpus = 1;
    single_config.cpu = effective_cpu.clone();
    let mut vm = DeterministicVm::new(single_config).expect("admit single-vCPU diagnostic base");
    let cpuid = vm.execution_descriptor.guest_cpuid_template().unwrap();
    for index in 0..num_vcpus {
        if index > 0 {
            vm.vcpus.push(vm.vm.create_vcpu(index as u64).unwrap());
        }
        let cpuid = cpu::patch_cpuid_apic_id(&cpuid, index as u32, num_vcpus as u32).unwrap();
        vm.vcpus[index].set_cpuid2(&cpuid).unwrap();
        cpu::setup_tsc(&vm.vcpus[index], effective_cpu.tsc_khz).unwrap();
    }
    let initial_runnable = vm
        .vcpus
        .iter()
        .enumerate()
        .map(|(index, fd)| DeterministicVm::vcpu_fd_is_runnable(index, fd).unwrap())
        .collect();
    vm.scheduler = VcpuScheduler::try_new_with_journal_limit(
        &SchedulerConfig {
            num_vcpus,
            progress_budget: config.smp_progress_budget,
            strategy: config.scheduling_strategy,
            seed: effective_cpu.seed,
        },
        ProgressMode::ExactSingleStep,
        initial_runnable,
        config.smp_schedule_journal_limit,
    )
    .unwrap();
    vm.active_vcpu = vm.scheduler.active();
    vm.hlt_latched_vcpus = vec![false; num_vcpus];
    vm.guest_determinism_profile = build_guest_determinism_profile(GuestDeterminismInput {
        run_seed: effective_cpu.seed,
        vm_id: config.vm_id.try_into().unwrap(),
        vcpu_count: num_vcpus.try_into().unwrap(),
        tsc_khz: effective_cpu.tsc_khz,
        clock_mode: GuestClockMode::DeterministicJiffies,
    });
    vm
}

// Production-admitted coarse microguests. Unlike partial_smp_vm these use the
// actual coarse constructor, bounded entry path and settled-exit classifier.
pub(crate) const COARSE_CODE_ADDRESS: u64 = 0x200;
pub(crate) const COARSE_HANDLER_ADDRESS: u64 = 0x300;
pub(crate) const COARSE_STACK_ADDRESS: u64 = 0x1000;
pub(crate) const COARSE_IRQ_VECTOR: u8 = 0x51;
pub(crate) const COARSE_WATCHDOG_US: u64 = 100_000;
pub(crate) const COARSE_TIMEOUT_WATCHDOG_US: u64 = 10_000;
pub(crate) const COARSE_MEMORY_BYTES: usize = 4 * 1024 * 1024;

pub(crate) fn coarse_smp_vm(
    code: &[u8],
    handler: &[u8],
    boundary_quantum: u64,
    watchdog_us: u64,
) -> DeterministicVm {
    let mut config = VmConfig {
        memory_size: COARSE_MEMORY_BYTES,
        num_vcpus: 2,
        smp_progress_mode: ProgressMode::KvmDebugBoundary,
        smp_progress_budget: ProgressBudget::KvmBoundaries {
            profile_id: BoundaryProfileId::KvmDebugExitBoundaryV1,
            count: boundary_quantum,
        },
        smp_entry_watchdog_microseconds: watchdog_us,
        ..VmConfig::default()
    };
    config
        .apply_kvm_debug_boundary_profile(BoundaryProfileId::KvmDebugExitBoundaryV1)
        .unwrap();
    let scheduler_config = SchedulerConfig {
        num_vcpus: config.num_vcpus,
        progress_budget: config.smp_progress_budget,
        strategy: config.scheduling_strategy,
        seed: config.cpu.seed,
    };
    let journal_limit = config.smp_schedule_journal_limit;
    let mut vm = DeterministicVm::new(config).expect("production coarse admission");
    vm.write_guest_memory(COARSE_CODE_ADDRESS, code).unwrap();
    vm.write_guest_memory(COARSE_HANDLER_ADDRESS, handler)
        .unwrap();
    let vector = [
        COARSE_HANDLER_ADDRESS as u8,
        (COARSE_HANDLER_ADDRESS >> 8) as u8,
        0,
        0,
    ];
    vm.write_guest_memory(u64::from(COARSE_IRQ_VECTOR) * 4, &vector)
        .unwrap();
    for (index, vcpu) in vm.vcpus.iter().enumerate() {
        let mut sregs = vcpu.get_sregs().unwrap();
        sregs.cs.base = 0;
        sregs.cs.selector = 0;
        sregs.ss.base = 0;
        sregs.ss.selector = 0;
        vcpu.set_sregs(&sregs).unwrap();
        vcpu.set_mp_state(kvm_bindings::kvm_mp_state {
            mp_state: kvm_bindings::KVM_MP_STATE_RUNNABLE,
        })
        .unwrap();
        vcpu.set_regs(&kvm_regs {
            rip: COARSE_CODE_ADDRESS,
            rsp: COARSE_STACK_ADDRESS + index as u64 * 0x100,
            rflags: 2 | (1 << 9),
            ..Default::default()
        })
        .unwrap();
    }
    vm.scheduler = VcpuScheduler::try_new_with_journal_limit(
        &scheduler_config,
        ProgressMode::KvmDebugBoundary,
        vec![true, true],
        journal_limit,
    )
    .unwrap();
    vm.active_vcpu = vm.scheduler.active();
    vm.configure_progress_source_for_current_turn().unwrap();
    vm
}

/// Seed the kernel's observable accepted-but-not-yet-delivered IRQ state.
/// This is not an injection-ledger request or a claim about its delivery RIP.
pub(crate) fn stage_coarse_irq(vm: &mut DeterministicVm) {
    let mut events = vm.vcpus[0].get_vcpu_events().unwrap();
    events.interrupt.injected = 1;
    events.interrupt.nr = COARSE_IRQ_VECTOR;
    events.interrupt.soft = 0;
    events.interrupt.shadow = 0;
    vm.vcpus[0].set_vcpu_events(&events).unwrap();
    let accepted = vm.vcpus[0].get_vcpu_events().unwrap();
    assert_eq!(accepted.interrupt.injected, 1);
    assert_eq!(accepted.interrupt.nr, COARSE_IRQ_VECTOR);
}
