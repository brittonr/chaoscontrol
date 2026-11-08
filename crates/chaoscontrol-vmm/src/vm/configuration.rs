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
        smp_progress_budget,
        smp_entry_watchdog_microseconds,
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
    assert_eq!(
        smp_progress_budget,
        ProgressBudget::Instructions(DEFAULT_SMP_INSTRUCTION_QUANTUM)
    );
    assert_eq!(
        smp_entry_watchdog_microseconds,
        DEFAULT_SMP_ENTRY_WATCHDOG_MICROSECONDS
    );
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
        tsc_mode,
    } = cpu;
    assert_eq!(tsc_khz, crate::cpu::DEFAULT_TSC_KHZ);
    assert_eq!(tsc_advance_per_tick, crate::cpu::DEFAULT_TSC_ADVANCE);
    assert_eq!(tsc_mode, crate::cpu::TscAdvanceMode::PerExit);
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

fn explicit_coarse_config() -> VmConfig {
    VmConfig {
        num_vcpus: 4,
        smp_progress_mode: ProgressMode::KvmDebugBoundary,
        smp_progress_budget: ProgressBudget::KvmBoundaries {
            profile_id: BoundaryProfileId::KvmDebugExitBoundaryV1,
            count: 13,
        },
        smp_entry_watchdog_microseconds: 170_000,
        smp_schedule_journal_limit: 4096,
        ..VmConfig::default()
    }
}

#[test]
// r[verify chaoscontrol.kvm_debug_boundary.admission]
fn explicitly_selected_profile_preserves_bounds_and_binds_declared_clock_and_cpu() {
    let mut config = explicit_coarse_config();
    config.extra_cmdline = Some("reviewed_workload=fixture".into());
    let budget = config.smp_progress_budget;
    let watchdog = config.smp_entry_watchdog_microseconds;
    let journal = config.smp_schedule_journal_limit;
    config
        .apply_kvm_debug_boundary_profile(BoundaryProfileId::KvmDebugExitBoundaryV1)
        .unwrap();
    assert_eq!(config.smp_progress_budget, budget);
    assert_eq!(config.smp_entry_watchdog_microseconds, watchdog);
    assert_eq!(config.smp_schedule_journal_limit, journal);
    assert!(config.cpu.hide_tsc && config.cpu.hide_hypervisor);
    assert_eq!(config.cpu.tsc_mode, crate::cpu::TscAdvanceMode::PerExit);
    let command = std::str::from_utf8(&config.cmdline[..config.cmdline.len() - 1]).unwrap();
    let extra = config.extra_cmdline.as_deref().unwrap();
    for token in ["noapictimer", "clocksource=jiffies", "notsc"] {
        assert_eq!(
            command
                .split_ascii_whitespace()
                .filter(|value| *value == token)
                .count(),
            1
        );
        assert_eq!(
            extra
                .split_ascii_whitespace()
                .filter(|value| *value == token)
                .count(),
            1
        );
    }
    assert!(extra.contains("reviewed_workload=fixture"));
    assert!(!command.contains("clocksource=tsc"));
    assert!(!command.contains("tsc=reliable"));
    let selected = serde_json::to_value(&config).unwrap();
    config
        .apply_kvm_debug_boundary_profile(BoundaryProfileId::KvmDebugExitBoundaryV1)
        .unwrap();
    assert_eq!(
        serde_json::to_value(&config).unwrap(),
        selected,
        "profile selection is idempotent"
    );
}

#[test]
// r[verify chaoscontrol.kvm_debug_boundary.admission]
fn conflicting_caller_clock_inputs_reject_without_partial_configuration_mutation() {
    for token in [
        "clocksource=tsc",
        "clocksource=hpet",
        "tsc=reliable",
        "clock=tsc",
        "hpet=force",
    ] {
        for use_extra in [false, true] {
            let mut config = explicit_coarse_config();
            if use_extra {
                config.extra_cmdline = Some(format!("reviewed_workload=fixture {token}"));
            } else {
                config.cmdline = format!("console=ttyS0 {token}\0").into_bytes();
            }
            let before = serde_json::to_value(&config).unwrap();
            assert!(matches!(
                config.apply_kvm_debug_boundary_profile(BoundaryProfileId::KvmDebugExitBoundaryV1),
                Err(VmError::ProgressCapability { .. })
            ));
            assert_eq!(
                serde_json::to_value(&config).unwrap(),
                before,
                "token={token}, extra={use_extra}"
            );
        }
    }
}

#[test]
// r[verify chaoscontrol.kvm_debug_boundary.admission]
fn profile_never_selects_coarse_mode_or_infers_units_or_bounds() {
    for invalid in 0..7 {
        let mut config = explicit_coarse_config();
        match invalid {
            0 => config.smp_progress_mode = ProgressMode::ExactSingleStep,
            1 => config.smp_progress_budget = ProgressBudget::Instructions(13),
            2 => config.smp_progress_budget = config.smp_progress_budget.with_count(0),
            3 => config.smp_entry_watchdog_microseconds = 0,
            4 => config.smp_entry_watchdog_microseconds = u64::MAX,
            5 => config.smp_schedule_journal_limit = 0,
            6 => config.num_vcpus = 1,
            _ => unreachable!(),
        }
        let before = serde_json::to_value(&config).unwrap();
        assert!(config
            .apply_kvm_debug_boundary_profile(BoundaryProfileId::KvmDebugExitBoundaryV1)
            .is_err());
        assert_eq!(serde_json::to_value(&config).unwrap(), before);
    }
}

#[test]
// r[verify chaoscontrol.kvm_debug_boundary.admission]
fn direct_admission_cannot_bypass_clock_profile_selection_or_conflict_checks() {
    let mut config = explicit_coarse_config();
    assert!(coarse_configuration::validate_coarse_clock_configuration(&config).is_err());
    config
        .apply_kvm_debug_boundary_profile(BoundaryProfileId::KvmDebugExitBoundaryV1)
        .unwrap();
    coarse_configuration::validate_coarse_clock_configuration(&config).unwrap();
    for conflict in ["clocksource=tsc", "tsc=reliable", "hpet=force"] {
        let mut extra_conflict = config.clone();
        extra_conflict.extra_cmdline.as_mut().unwrap().push(' ');
        extra_conflict
            .extra_cmdline
            .as_mut()
            .unwrap()
            .push_str(conflict);
        assert!(
            coarse_configuration::validate_coarse_clock_configuration(&extra_conflict).is_err()
        );
        let mut command_conflict = config.clone();
        command_conflict.cmdline = format!("console=ttyS0 {conflict}\0").into_bytes();
        assert!(
            coarse_configuration::validate_coarse_clock_configuration(&command_conflict).is_err()
        );
    }
    for omitted in ["noapictimer", "clocksource=jiffies", "notsc"] {
        let mut incomplete = config.clone();
        incomplete.extra_cmdline = Some(
            ["noapictimer", "clocksource=jiffies", "notsc"]
                .into_iter()
                .filter(|token| *token != omitted)
                .collect::<Vec<_>>()
                .join(" "),
        );
        assert!(coarse_configuration::validate_coarse_clock_configuration(&incomplete).is_err());
    }
}
