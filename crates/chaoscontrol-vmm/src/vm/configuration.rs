//! Configuration regression checks without KVM or filesystem effects.

use super::*;

const EXPECTED_MEMORY_BYTES: usize = 268_435_456;
const EXPECTED_FAMILY: u8 = 6;
const EXPECTED_MODEL: u8 = 85;
const EXPECTED_STEPPING: u8 = 4;
const EXPECTED_BOOT_COMMAND: &[u8] = b"console=ttyS0 earlyprintk=serial \
    clocksource=tsc tsc=reliable \
    lpj=6000000 \
    nokaslr noapic nosmp \
    nohpet \
    randomize_kstack_offset=off norandmaps \
    random.trust_cpu=off random.trust_bootloader=off \
    kfence.sample_interval=0 \
    no_hash_pointers \
    virtio_mmio.device=4K@0xd0000000:5 \
    virtio_mmio.device=4K@0xd0001000:6 \
    virtio_mmio.device=4K@0xd0002000:7 \
    panic=0\0";

fn assert_configuration(config: VmConfig, expected_seed: u64, expected_hide_tsc: bool) {
    // Exhaustive destructuring makes new configuration fields require review.
    let VmConfig {
        memory_size,
        cpu,
        num_vcpus,
        scheduling_strategy,
        smp_progress_mode,
        smp_instruction_quantum,
        smp_schedule_journal_limit,
        cmdline,
        disk_image_path,
        extra_cmdline,
        core_affinity,
        vm_id,
        dlog_path,
        dlog_register_interval,
        dlog_memory_hash,
    } = config;
    assert_eq!(memory_size, EXPECTED_MEMORY_BYTES);
    assert_eq!(num_vcpus, 1);
    assert!(matches!(
        scheduling_strategy,
        SchedulingStrategy::RoundRobin
    ));
    assert_eq!(smp_progress_mode, ProgressMode::ExactSingleStep);
    assert_eq!(smp_instruction_quantum, DEFAULT_SMP_INSTRUCTION_QUANTUM);
    assert_eq!(smp_schedule_journal_limit, DEFAULT_SCHEDULE_JOURNAL_LIMIT);
    assert_eq!(cmdline, EXPECTED_BOOT_COMMAND);
    assert_eq!(vm_id, 0);
    assert!(disk_image_path.is_none());
    assert!(extra_cmdline.is_none());
    assert!(core_affinity.is_none());
    assert!(dlog_path.is_none());
    assert_eq!(dlog_register_interval, 0);
    assert!(!dlog_memory_hash);
    assert_cpu(cpu, expected_seed, expected_hide_tsc);
}

fn assert_cpu(cpu: CpuConfig, expected_seed: u64, expected_hide_tsc: bool) {
    let CpuConfig {
        tsc_khz,
        allow_avx2,
        allow_avx512,
        hide_hypervisor,
        hide_tsc,
        fixed_family,
        fixed_model,
        fixed_stepping,
        fixed_frequency_mhz,
        seed,
        tsc_advance_per_tick,
    } = cpu;
    assert_eq!(tsc_khz, crate::cpu::DEFAULT_TSC_KHZ);
    assert_eq!(tsc_advance_per_tick, crate::cpu::DEFAULT_TSC_ADVANCE);
    assert_eq!(seed, expected_seed);
    assert_eq!(hide_tsc, expected_hide_tsc);
    assert!(hide_hypervisor);
    assert!(!allow_avx2);
    assert!(!allow_avx512);
    assert_eq!(fixed_family, Some(EXPECTED_FAMILY));
    assert_eq!(fixed_model, Some(EXPECTED_MODEL));
    assert_eq!(fixed_stepping, Some(EXPECTED_STEPPING));
    assert!(fixed_frequency_mhz.is_none());
}

#[test]
fn compatibility_default_preserves_every_field() {
    assert_configuration(VmConfig::default(), 0, false);
}

#[test]
fn hidden_tsc_profile_preserves_full_width_seed_and_disables_optional_effects() {
    assert_configuration(VmConfig::single_vcpu(u64::MAX, true), u64::MAX, true);
}

#[test]
fn visible_tsc_profile_does_not_inherit_hidden_tsc_or_another_seed() {
    let hidden = VmConfig::single_vcpu(u64::MAX, true);
    let visible = VmConfig::single_vcpu(1, false);
    assert_configuration(hidden, u64::MAX, true);
    assert_configuration(visible, 1, false);
}
