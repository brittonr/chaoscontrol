use chaoscontrol_fault::schedule;
use chaoscontrol_vmm::{controller, cpu, scheduler, vm};

pub const VM_COUNT: usize = 2;
pub const TICKS: u64 = 1;
const MEMORY_BYTES: usize = 0x0080_0000;
const JOURNAL_RECORDS: usize = 128;
const EXIT_QUANTUM: u64 = super::guest::MAXIMUM_EXITS;
const SEED: u64 = 31;

pub fn config(kernel: &std::path::Path) -> controller::SimulationConfig {
    controller::SimulationConfig {
        num_vms: VM_COUNT,
        vm_config: vm::VmConfig {
            memory_size: MEMORY_BYTES,
            cpu: cpu::CpuConfig {
                tsc_khz: cpu::DEFAULT_TSC_KHZ,
                allow_avx2: false,
                allow_avx512: false,
                hide_hypervisor: true,
                hide_tsc: true,
                fixed_family: None,
                fixed_model: None,
                fixed_stepping: None,
                fixed_frequency_mhz: None,
                seed: SEED,
                tsc_advance_per_tick: cpu::DEFAULT_TSC_ADVANCE,
            },
            num_vcpus: 1,
            scheduling_strategy: scheduler::SchedulingStrategy::RoundRobin,
            smp_progress_mode: scheduler::core::ProgressMode::ExactSingleStep,
            smp_instruction_quantum: 1,
            smp_schedule_journal_limit: JOURNAL_RECORDS,
            cmdline: b"\0".to_vec(),
            disk_image_path: None,
            extra_cmdline: None,
            core_affinity: None,
            vm_id: 0,
            dlog_path: None,
            dlog_register_interval: 0,
            dlog_memory_hash: false,
        },
        kernel_path: kernel.to_str().unwrap().to_string(),
        initrd_path: None,
        seed: SEED,
        quantum: EXIT_QUANTUM,
        schedule: schedule::FaultSchedule::new(),
        disk_image_path: None,
        bootstrap_budget: Some(TICKS),
        base_core: None,
        dlog_dir: None,
    }
}
