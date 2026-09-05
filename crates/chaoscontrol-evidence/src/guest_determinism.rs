//! Imperative guest determinism probe shell.
//!
//! The shell runs a dedicated guest fixture and supplies observations to the
//! pure drift decision in `chaoscontrol-sim-core`.

pub const GUEST_PROBE_PREFIX: &str = "GUEST_DETERMINISM_PROBE=";
pub const GUEST_PROBE_READY: &str = "GUEST_DETERMINISM_READY";
pub const GUEST_PROBE_DONE: &str = "chaoscontrol-guest-determinism-probe: done";

// Pin the existing probe cohort. The compatibility test detects VMM default drift.
const PROBE_MEMORY_BYTES: usize = 0x1000_0000;
const PROBE_CPU_FAMILY: u8 = 6;
const PROBE_CPU_MODEL: u8 = 85;
const PROBE_CPU_STEPPING: u8 = 4;
const PROBE_CMDLINE: &[u8] = b"console=ttyS0 earlyprintk=serial \
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

fn probe_config(run_seed: u64) -> ::chaoscontrol_vmm::vm::VmConfig {
    ::chaoscontrol_vmm::vm::VmConfig {
        memory_size: PROBE_MEMORY_BYTES,
        num_vcpus: 1,
        scheduling_strategy: chaoscontrol_vmm::scheduler::SchedulingStrategy::RoundRobin,
        smp_progress_mode: chaoscontrol_sim_core::scheduler::core::ProgressMode::ExactSingleStep,
        smp_instruction_quantum: chaoscontrol_sim_core::scheduler::DEFAULT_SMP_INSTRUCTION_QUANTUM,
        smp_schedule_journal_limit:
            chaoscontrol_sim_core::scheduler::core::DEFAULT_SCHEDULE_JOURNAL_LIMIT,
        cpu: chaoscontrol_vmm::cpu::CpuConfig {
            tsc_khz: chaoscontrol_vmm::cpu::DEFAULT_TSC_KHZ,
            allow_avx2: false,
            allow_avx512: false,
            hide_hypervisor: true,
            hide_tsc: true,
            fixed_family: Some(PROBE_CPU_FAMILY),
            fixed_model: Some(PROBE_CPU_MODEL),
            fixed_stepping: Some(PROBE_CPU_STEPPING),
            fixed_frequency_mhz: None,
            seed: run_seed,
            tsc_advance_per_tick: chaoscontrol_vmm::cpu::DEFAULT_TSC_ADVANCE,
        },
        cmdline: PROBE_CMDLINE.to_vec(),
        disk_image_path: None,
        extra_cmdline: None,
        core_affinity: None,
        vm_id: 0,
        dlog_path: None,
        dlog_register_interval: 0,
        dlog_memory_hash: false,
    }
}

#[derive(Debug)]
pub enum GuestDeterminismShellError {
    Vm(String),
    MissingProbe,
    DuplicateProbe,
    MalformedProbe(String),
    ProfileDrift,
    DriftDecision(String),
    Io(String),
    Serialization(String),
}

/// Execute one fixture guest and return its admitted profile and observation.
pub fn run_guest_determinism_probe(
    kernel: &std::path::Path,
    initrd: &std::path::Path,
    run_seed: u64,
) -> Result<
    (
        ::chaoscontrol_sim_core::GuestDeterminismProfile,
        ::chaoscontrol_sim_core::GuestDeterminismProbe,
    ),
    GuestDeterminismShellError,
> {
    let mut vm = ::chaoscontrol_vmm::vm::DeterministicVm::new(probe_config(run_seed))
        .map_err(|error| GuestDeterminismShellError::Vm(error.to_string()))?;
    vm.load_kernel(&kernel.to_string_lossy(), Some(&initrd.to_string_lossy()))
        .map_err(|error| GuestDeterminismShellError::Vm(error.to_string()))?;
    let profile = vm.guest_determinism_profile().clone();
    let serial = vm
        .run_until(GUEST_PROBE_DONE)
        .map_err(|error| GuestDeterminismShellError::Vm(error.to_string()))?;
    let probe = extract_guest_determinism_probe(&serial)?;
    Ok((profile, probe))
}

/// Parse one exact probe record from bounded serial output.
pub fn extract_guest_determinism_probe(
    serial: &str,
) -> Result<::chaoscontrol_sim_core::GuestDeterminismProbe, GuestDeterminismShellError> {
    let mut matches = serial.lines().filter_map(|line| {
        line.trim()
            .strip_prefix(GUEST_PROBE_PREFIX)
            .map(str::to_string)
    });
    let encoded = matches
        .next()
        .ok_or(GuestDeterminismShellError::MissingProbe)?;
    if matches.next().is_some() {
        return Err(GuestDeterminismShellError::DuplicateProbe);
    }
    serde_json::from_str(&encoded)
        .map_err(|error| GuestDeterminismShellError::MalformedProbe(error.to_string()))
}

/// Compare two continuations from one admitted, quiescent guest snapshot.
pub fn run_guest_determinism_gate(
    kernel: &std::path::Path,
    initrd: &std::path::Path,
    run_seed: u64,
) -> Result<::chaoscontrol_sim_core::GuestDeterminismDriftReport, GuestDeterminismShellError> {
    let mut vm = ::chaoscontrol_vmm::vm::DeterministicVm::new(probe_config(run_seed))
        .map_err(|error| GuestDeterminismShellError::Vm(error.to_string()))?;
    vm.load_kernel(&kernel.to_string_lossy(), Some(&initrd.to_string_lossy()))
        .map_err(|error| GuestDeterminismShellError::Vm(error.to_string()))?;
    vm.run_until(GUEST_PROBE_READY)
        .map_err(|error| GuestDeterminismShellError::Vm(error.to_string()))?;
    let profile = vm.guest_determinism_profile().clone();
    let snapshot = vm
        .snapshot()
        .map_err(|error| GuestDeterminismShellError::Vm(error.to_string()))?;

    let left_serial = vm
        .run_until(GUEST_PROBE_DONE)
        .map_err(|error| GuestDeterminismShellError::Vm(error.to_string()))?;
    let left_probe = extract_guest_determinism_probe(&left_serial)?;

    vm.restore(&snapshot)
        .map_err(|error| GuestDeterminismShellError::Vm(error.to_string()))?;
    let right_serial = vm
        .run_until(GUEST_PROBE_DONE)
        .map_err(|error| GuestDeterminismShellError::Vm(error.to_string()))?;
    let right_probe = extract_guest_determinism_probe(&right_serial)?;

    ::chaoscontrol_sim_core::compare_guest_determinism_probes(&profile, &left_probe, &right_probe)
        .map_err(|error| GuestDeterminismShellError::DriftDecision(format!("{error:?}")))
}

/// Persist one canonical JSON report after the caller authorizes the path.
pub fn write_guest_determinism_report(
    path: &std::path::Path,
    report: &::chaoscontrol_sim_core::GuestDeterminismDriftReport,
) -> Result<(), GuestDeterminismShellError> {
    let bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| GuestDeterminismShellError::Serialization(error.to_string()))?;
    std::fs::write(path, bytes).map_err(|error| GuestDeterminismShellError::Io(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chaoscontrol_sim_core::{BOOT_ENTROPY_SEED_BYTES, GUEST_DETERMINISM_PROBE_SCHEMA};

    fn encoded_probe() -> String {
        let probe = ::chaoscontrol_sim_core::GuestDeterminismProbe {
            schema: GUEST_DETERMINISM_PROBE_SCHEMA.to_string(),
            entropy_hex: "ab".repeat(BOOT_ENTROPY_SEED_BYTES),
            monotonic_delta_ns: 1,
            text_address: 1,
            stack_address: 1,
            heap_address: 1,
            signal_order: vec![libc::SIGUSR1 as u32, libc::SIGUSR2 as u32],
        };
        serde_json::to_string(&probe).expect("serialize fixture")
    }

    #[test]
    fn explicit_probe_cohort_preserves_the_previous_configuration() {
        let mut previous = ::chaoscontrol_vmm::vm::VmConfig::default();
        previous.cpu.seed = u64::MAX;
        previous.cpu.hide_tsc = true;
        // Both types derive Debug over every field, including the nested CPU configuration.
        assert_eq!(
            format!("{previous:?}"),
            format!("{:?}", probe_config(u64::MAX))
        );
    }

    #[test]
    fn pins_probe_resources_without_truncating_the_seed() {
        let first = probe_config(0);
        let last = probe_config(u64::MAX);
        assert_eq!(first.memory_size, PROBE_MEMORY_BYTES);
        assert_eq!(last.memory_size, first.memory_size);
        assert_eq!(first.num_vcpus, 1);
        assert_eq!(last.num_vcpus, first.num_vcpus);
        assert_eq!(first.cpu.seed, 0);
        assert_eq!(last.cpu.seed, u64::MAX);
        assert_ne!(first.cpu.seed, last.cpu.seed);
        assert!(first.cpu.hide_tsc);
        assert!(last.cpu.hide_tsc);
    }

    #[test]
    fn extracts_one_complete_probe_line() {
        let serial = format!(
            "boot\n{GUEST_PROBE_PREFIX}{}\n{GUEST_PROBE_DONE}\n",
            encoded_probe()
        );
        assert!(extract_guest_determinism_probe(&serial).is_ok());
    }

    #[test]
    fn rejects_missing_duplicate_and_malformed_probe_lines() {
        assert!(matches!(
            extract_guest_determinism_probe("boot only"),
            Err(GuestDeterminismShellError::MissingProbe)
        ));
        let duplicate = format!(
            "{GUEST_PROBE_PREFIX}{}\n{GUEST_PROBE_PREFIX}{}\n",
            encoded_probe(),
            encoded_probe()
        );
        assert!(matches!(
            extract_guest_determinism_probe(&duplicate),
            Err(GuestDeterminismShellError::DuplicateProbe)
        ));
        assert!(matches!(
            extract_guest_determinism_probe(&format!("{GUEST_PROBE_PREFIX}{{bad}}")),
            Err(GuestDeterminismShellError::MalformedProbe(_))
        ));
    }
}
