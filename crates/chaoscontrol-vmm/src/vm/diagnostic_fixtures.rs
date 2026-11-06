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
    let cpuid = cpu::filter_cpuid(&vm.kvm, &effective_cpu).unwrap();
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
            quantum: config.smp_instruction_quantum,
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
