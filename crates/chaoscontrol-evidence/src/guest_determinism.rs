//! Imperative guest determinism probe shell.
//!
//! The shell runs a dedicated guest fixture and supplies observations to the
//! pure drift decision in `chaoscontrol-sim-core`.

use chaoscontrol_sim_core::{
    compare_guest_determinism_probes, GuestDeterminismDriftReport, GuestDeterminismProbe,
    GuestDeterminismProfile,
};
use chaoscontrol_vmm::vm::{DeterministicVm, VmConfig};
use std::path::Path;

pub const GUEST_PROBE_PREFIX: &str = "GUEST_DETERMINISM_PROBE=";
pub const GUEST_PROBE_READY: &str = "GUEST_DETERMINISM_READY";
pub const GUEST_PROBE_DONE: &str = "chaoscontrol-guest-determinism-probe: done";

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
    kernel: &Path,
    initrd: &Path,
    run_seed: u64,
) -> Result<(GuestDeterminismProfile, GuestDeterminismProbe), GuestDeterminismShellError> {
    let config = VmConfig::single_vcpu(run_seed, true);
    let mut vm = DeterministicVm::new(config)
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
) -> Result<GuestDeterminismProbe, GuestDeterminismShellError> {
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
    crate::json_preflight::preflight_json(&encoded, crate::json_preflight::QUALITY_REPORT_LIMITS)
        .map_err(|error| GuestDeterminismShellError::MalformedProbe(error.message().to_string()))?;
    serde_json::from_str(&encoded)
        .map_err(|error| GuestDeterminismShellError::MalformedProbe(error.to_string()))
}

/// Compare two continuations from one admitted, quiescent guest snapshot.
pub fn run_guest_determinism_gate(
    kernel: &Path,
    initrd: &Path,
    run_seed: u64,
) -> Result<GuestDeterminismDriftReport, GuestDeterminismShellError> {
    let config = VmConfig::single_vcpu(run_seed, true);
    let mut vm = DeterministicVm::new(config)
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

    compare_guest_determinism_probes(&profile, &left_probe, &right_probe)
        .map_err(|error| GuestDeterminismShellError::DriftDecision(format!("{error:?}")))
}

/// Persist one canonical JSON report after the caller authorizes the path.
pub fn write_guest_determinism_report(
    path: &Path,
    report: &GuestDeterminismDriftReport,
) -> Result<(), GuestDeterminismShellError> {
    let bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| GuestDeterminismShellError::Serialization(error.to_string()))?;
    crate::bounded_file::write_bytes_replace(path, &bytes)
        .map_err(|error| GuestDeterminismShellError::Io(error.message().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chaoscontrol_sim_core::{BOOT_ENTROPY_SEED_BYTES, GUEST_DETERMINISM_PROBE_SCHEMA};

    fn encoded_probe() -> String {
        let probe = GuestDeterminismProbe {
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
    fn extracts_one_complete_probe_line() {
        let serial = format!(
            "boot\n{GUEST_PROBE_PREFIX}{}\n{GUEST_PROBE_DONE}\n",
            encoded_probe()
        );
        assert!(extract_guest_determinism_probe(&serial).is_ok());
    }

    #[test]
    fn report_writer_creates_nested_output() {
        use chaoscontrol_sim_core::{
            build_guest_determinism_profile, GuestClockMode, GuestDeterminismInput,
        };

        let profile = build_guest_determinism_profile(GuestDeterminismInput {
            run_seed: 1,
            vm_id: 0,
            vcpu_count: 1,
            tsc_khz: 1_000,
            clock_mode: GuestClockMode::VirtualTsc,
        });
        let probe = GuestDeterminismProbe {
            schema: GUEST_DETERMINISM_PROBE_SCHEMA.to_string(),
            entropy_hex: "ab".repeat(BOOT_ENTROPY_SEED_BYTES),
            monotonic_delta_ns: 1,
            text_address: 1,
            stack_address: 1,
            heap_address: 1,
            signal_order: vec![libc::SIGUSR1 as u32, libc::SIGUSR2 as u32],
        };
        let report =
            chaoscontrol_sim_core::compare_guest_determinism_probes(&profile, &probe, &probe)
                .unwrap();
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("nested/report.json");
        write_guest_determinism_report(&path, &report).unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(value["schema"], report.schema);
    }

    #[test]
    fn rejects_excessively_nested_probe_lines() {
        let serial = format!(
            "{GUEST_PROBE_PREFIX}{}{}\n",
            "[".repeat(512),
            "]".repeat(512)
        );
        let error = extract_guest_determinism_probe(&serial).unwrap_err();
        let GuestDeterminismShellError::MalformedProbe(message) = error else {
            panic!("unexpected error: {error:?}");
        };
        assert!(message.contains("JSON nesting depth exceeded"));
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
