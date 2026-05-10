//! Determinism drift-gate receipt helpers.
//!
//! This module is intentionally KVM-free: the VM runner supplies fingerprints,
//! while this pure layer classifies mismatches and emits a machine-readable
//! receipt that CI/operator tooling can archive.

use serde::{Deserialize, Serialize};

/// Stable fingerprint for one VM run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmFingerprint {
    /// Number of VM exits observed before the run stopped.
    pub exit_count: u64,
    /// Final virtual TSC value.
    pub virtual_tsc: u64,
    /// Serial output after documented nondeterministic lines are stripped.
    pub serial_stripped: String,
}

/// Stable fingerprint for one multi-VM controller run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControllerFingerprint {
    /// Final controller tick.
    pub tick: u64,
    /// Per-VM exit counts.
    pub vm_exits: Vec<u64>,
    /// Per-VM virtual TSC values.
    pub vm_vtscs: Vec<u64>,
    /// Reserved for controller-level serial capture when exposed.
    pub serials_stripped: Vec<String>,
}

/// Fingerprint shape used by the generic comparison/reporting layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RunFingerprint {
    SingleVm(VmFingerprint),
    Controller(ControllerFingerprint),
}

/// One run inside a determinism case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunObservation {
    pub run_index: usize,
    pub fingerprint: RunFingerprint,
    /// Optional path to the dlog file or directory for this run.
    pub dlog_path: Option<String>,
}

/// Machine-readable mismatch details.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MismatchDetail {
    pub run_index: usize,
    pub field: String,
    pub expected: String,
    pub actual: String,
    #[serde(default)]
    pub class: DivergenceClass,
}

/// Coarse first-divergence classes used to make drift receipts actionable.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DivergenceClass {
    FingerprintCounters,
    SerialByteStream,
    PciConfigAccess,
    RtcCmosAccess,
    PitTscCalibration,
    VcpuScheduling,
    MmioDeviceAccess,
    DeviceIoAccess,
    DlogLength,
    DlogCompareError,
    FingerprintKind,
    #[default]
    Unknown,
}

/// Machine-readable dlog first-divergence detail.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DlogDivergenceDetail {
    pub run_index: usize,
    pub class: DivergenceClass,
    pub summary: String,
}

/// Result for one deterministic configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeterminismCaseReport {
    pub name: String,
    pub runs: usize,
    pub passed: bool,
    pub reference: RunFingerprint,
    pub observations: Vec<RunObservation>,
    pub mismatches: Vec<MismatchDetail>,
    /// True when dlog structural comparison was requested and matched for all
    /// non-reference runs. None means the run did not emit dlogs.
    pub dlog_structural_match: Option<bool>,
    /// Machine-readable dlog structural mismatch summaries. Empty means no dlog
    /// comparison failed or dlogs were not requested.
    pub dlog_mismatches: Vec<String>,
    /// Structured dlog first-divergence details. Mirrors `dlog_mismatches`
    /// with a stable class for downstream summaries.
    #[serde(default)]
    pub dlog_divergences: Vec<DlogDivergenceDetail>,
    /// Unique fingerprint and dlog first-divergence classes observed in this
    /// case, in first-seen order.
    #[serde(default)]
    pub divergence_classes: Vec<DivergenceClass>,
}

/// Top-level receipt for a determinism drift-gate run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeterminismGateReceipt {
    pub schema_version: u32,
    pub gate: String,
    pub kernel_path: String,
    pub initrd_path: String,
    pub kernel_crc32: String,
    pub initrd_crc32: String,
    pub cases: Vec<DeterminismCaseReport>,
    pub passed: bool,
}

impl DeterminismGateReceipt {
    /// Build a receipt and compute the aggregate status from all cases.
    pub fn new(
        kernel_path: String,
        initrd_path: String,
        kernel_crc32: String,
        initrd_crc32: String,
        cases: Vec<DeterminismCaseReport>,
    ) -> Self {
        let passed = cases.iter().all(|case| case.passed);
        Self {
            schema_version: 1,
            gate: "vm-determinism-drift".to_string(),
            kernel_path,
            initrd_path,
            kernel_crc32,
            initrd_crc32,
            cases,
            passed,
        }
    }
}

/// Build a case report by comparing every observation against the first run.
pub fn compare_case(
    name: impl Into<String>,
    observations: Vec<RunObservation>,
) -> DeterminismCaseReport {
    assert!(
        !observations.is_empty(),
        "determinism case requires at least one observation"
    );
    let reference = observations[0].fingerprint.clone();
    let mut mismatches = Vec::new();

    for observation in observations.iter().skip(1) {
        mismatches.extend(compare_fingerprints(
            observation.run_index,
            &reference,
            &observation.fingerprint,
        ));
    }

    let divergence_classes = unique_mismatch_classes(&mismatches);

    DeterminismCaseReport {
        name: name.into(),
        runs: observations.len(),
        passed: mismatches.is_empty(),
        reference,
        observations,
        mismatches,
        dlog_structural_match: None,
        dlog_mismatches: Vec::new(),
        dlog_divergences: Vec::new(),
        divergence_classes,
    }
}

fn compare_fingerprints(
    run_index: usize,
    expected: &RunFingerprint,
    actual: &RunFingerprint,
) -> Vec<MismatchDetail> {
    match (expected, actual) {
        (RunFingerprint::SingleVm(a), RunFingerprint::SingleVm(b)) => {
            compare_single_vm(run_index, a, b)
        }
        (RunFingerprint::Controller(a), RunFingerprint::Controller(b)) => {
            compare_controller(run_index, a, b)
        }
        _ => vec![MismatchDetail {
            run_index,
            field: "kind".to_string(),
            expected: fingerprint_kind(expected).to_string(),
            actual: fingerprint_kind(actual).to_string(),
            class: DivergenceClass::FingerprintKind,
        }],
    }
}

fn compare_single_vm(
    run_index: usize,
    expected: &VmFingerprint,
    actual: &VmFingerprint,
) -> Vec<MismatchDetail> {
    let mut mismatches = Vec::new();
    push_if_ne(
        &mut mismatches,
        run_index,
        "exit_count",
        expected.exit_count,
        actual.exit_count,
    );
    push_if_ne(
        &mut mismatches,
        run_index,
        "virtual_tsc",
        expected.virtual_tsc,
        actual.virtual_tsc,
    );
    if expected.serial_stripped != actual.serial_stripped {
        mismatches.push(MismatchDetail {
            run_index,
            field: "serial_stripped".to_string(),
            expected: first_diff_line(&expected.serial_stripped, &actual.serial_stripped)
                .0
                .unwrap_or_else(|| format!("{} bytes", expected.serial_stripped.len())),
            actual: first_diff_line(&expected.serial_stripped, &actual.serial_stripped)
                .1
                .unwrap_or_else(|| format!("{} bytes", actual.serial_stripped.len())),
            class: classify_serial_mismatch(&expected.serial_stripped, &actual.serial_stripped),
        });
    }
    mismatches
}

fn compare_controller(
    run_index: usize,
    expected: &ControllerFingerprint,
    actual: &ControllerFingerprint,
) -> Vec<MismatchDetail> {
    let mut mismatches = Vec::new();
    push_if_ne(
        &mut mismatches,
        run_index,
        "tick",
        expected.tick,
        actual.tick,
    );
    push_vec_if_ne(
        &mut mismatches,
        run_index,
        "vm_exits",
        &expected.vm_exits,
        &actual.vm_exits,
    );
    push_vec_if_ne(
        &mut mismatches,
        run_index,
        "vm_vtscs",
        &expected.vm_vtscs,
        &actual.vm_vtscs,
    );
    push_vec_if_ne(
        &mut mismatches,
        run_index,
        "serials_stripped",
        &expected.serials_stripped,
        &actual.serials_stripped,
    );
    mismatches
}

fn push_if_ne<T>(
    mismatches: &mut Vec<MismatchDetail>,
    run_index: usize,
    field: &str,
    expected: T,
    actual: T,
) where
    T: PartialEq + std::fmt::Display,
{
    if expected != actual {
        mismatches.push(MismatchDetail {
            run_index,
            field: field.to_string(),
            expected: expected.to_string(),
            actual: actual.to_string(),
            class: DivergenceClass::FingerprintCounters,
        });
    }
}

fn push_vec_if_ne<T>(
    mismatches: &mut Vec<MismatchDetail>,
    run_index: usize,
    field: &str,
    expected: &[T],
    actual: &[T],
) where
    T: PartialEq + std::fmt::Debug,
{
    if expected != actual {
        mismatches.push(MismatchDetail {
            run_index,
            field: field.to_string(),
            expected: format!("{expected:?}"),
            actual: format!("{actual:?}"),
            class: DivergenceClass::FingerprintCounters,
        });
    }
}

fn fingerprint_kind(fingerprint: &RunFingerprint) -> &'static str {
    match fingerprint {
        RunFingerprint::SingleVm(_) => "single-vm",
        RunFingerprint::Controller(_) => "controller",
    }
}

/// Classify a serial-output mismatch by the first differing line. This keeps
/// the classifier pure and usable without KVM/dlog artifacts.
pub fn classify_serial_mismatch(expected: &str, actual: &str) -> DivergenceClass {
    let (expected_line, actual_line) = first_diff_line(expected, actual);
    let text = format!(
        "{} {}",
        expected_line.as_deref().unwrap_or_default(),
        actual_line.as_deref().unwrap_or_default()
    )
    .to_ascii_lowercase();

    if text.contains("tsc") || text.contains("pit") || text.contains("clocksource") {
        DivergenceClass::PitTscCalibration
    } else {
        DivergenceClass::SerialByteStream
    }
}

/// Recompute aggregate first-divergence classes after optional dlog comparison.
pub fn refresh_divergence_classes(report: &mut DeterminismCaseReport) {
    report.divergence_classes = unique_mismatch_classes(&report.mismatches);
    for detail in &report.dlog_divergences {
        if !report.divergence_classes.contains(&detail.class) {
            report.divergence_classes.push(detail.class);
        }
    }
}

fn unique_mismatch_classes(mismatches: &[MismatchDetail]) -> Vec<DivergenceClass> {
    let mut classes = Vec::new();
    for mismatch in mismatches {
        if !classes.contains(&mismatch.class) {
            classes.push(mismatch.class);
        }
    }
    classes
}

fn first_diff_line(expected: &str, actual: &str) -> (Option<String>, Option<String>) {
    for (idx, (a, b)) in expected.lines().zip(actual.lines()).enumerate() {
        if a != b {
            return (
                Some(format!("line {idx}: {a}")),
                Some(format!("line {idx}: {b}")),
            );
        }
    }
    if expected.lines().count() != actual.lines().count() {
        return (
            Some(format!("{} lines", expected.lines().count())),
            Some(format!("{} lines", actual.lines().count())),
        );
    }
    (None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_single_vm_case_passes() {
        let fp = RunFingerprint::SingleVm(VmFingerprint {
            exit_count: 10,
            virtual_tsc: 20,
            serial_stripped: "ready".to_string(),
        });
        let report = compare_case(
            "single",
            vec![
                RunObservation {
                    run_index: 1,
                    fingerprint: fp.clone(),
                    dlog_path: None,
                },
                RunObservation {
                    run_index: 2,
                    fingerprint: fp,
                    dlog_path: None,
                },
            ],
        );
        assert!(report.passed);
        assert!(report.mismatches.is_empty());
    }

    #[test]
    fn single_vm_mismatch_reports_field() {
        let report = compare_case(
            "single",
            vec![
                RunObservation {
                    run_index: 1,
                    fingerprint: RunFingerprint::SingleVm(VmFingerprint {
                        exit_count: 10,
                        virtual_tsc: 20,
                        serial_stripped: "ready".to_string(),
                    }),
                    dlog_path: None,
                },
                RunObservation {
                    run_index: 2,
                    fingerprint: RunFingerprint::SingleVm(VmFingerprint {
                        exit_count: 11,
                        virtual_tsc: 20,
                        serial_stripped: "ready".to_string(),
                    }),
                    dlog_path: None,
                },
            ],
        );
        assert!(!report.passed);
        assert_eq!(report.mismatches[0].field, "exit_count");
        assert_eq!(
            report.mismatches[0].class,
            DivergenceClass::FingerprintCounters
        );
        assert_eq!(
            report.divergence_classes,
            vec![DivergenceClass::FingerprintCounters]
        );
    }

    #[test]
    fn serial_tsc_calibration_mismatch_is_classified() {
        let report = compare_case(
            "single",
            vec![
                RunObservation {
                    run_index: 1,
                    fingerprint: RunFingerprint::SingleVm(VmFingerprint {
                        exit_count: 10,
                        virtual_tsc: 20,
                        serial_stripped: "boot\ntsc: Fast TSC calibration using PIT".to_string(),
                    }),
                    dlog_path: None,
                },
                RunObservation {
                    run_index: 2,
                    fingerprint: RunFingerprint::SingleVm(VmFingerprint {
                        exit_count: 10,
                        virtual_tsc: 20,
                        serial_stripped: "boot\ntsc: Fast TSC calibration failed".to_string(),
                    }),
                    dlog_path: None,
                },
            ],
        );
        assert_eq!(report.mismatches[0].field, "serial_stripped");
        assert_eq!(
            report.mismatches[0].class,
            DivergenceClass::PitTscCalibration
        );
        assert_eq!(
            report.divergence_classes,
            vec![DivergenceClass::PitTscCalibration]
        );
    }

    #[test]
    fn refresh_divergence_classes_appends_dlog_classes() {
        let fp = RunFingerprint::SingleVm(VmFingerprint {
            exit_count: 10,
            virtual_tsc: 20,
            serial_stripped: "ready".to_string(),
        });
        let mut report = compare_case(
            "single",
            vec![RunObservation {
                run_index: 1,
                fingerprint: fp,
                dlog_path: None,
            }],
        );
        report.dlog_divergences.push(DlogDivergenceDetail {
            run_index: 2,
            class: DivergenceClass::PciConfigAccess,
            summary: "run 2 dlog structural mismatch".to_string(),
        });
        refresh_divergence_classes(&mut report);
        assert_eq!(
            report.divergence_classes,
            vec![DivergenceClass::PciConfigAccess]
        );
    }

    #[test]
    fn receipt_aggregates_case_status() {
        let passing = compare_case(
            "controller",
            vec![RunObservation {
                run_index: 1,
                fingerprint: RunFingerprint::Controller(ControllerFingerprint {
                    tick: 1,
                    vm_exits: vec![1, 2],
                    vm_vtscs: vec![3, 4],
                    serials_stripped: vec![],
                }),
                dlog_path: None,
            }],
        );
        let receipt = DeterminismGateReceipt::new(
            "kernel".to_string(),
            "initrd".to_string(),
            "crc32:1".to_string(),
            "crc32:2".to_string(),
            vec![passing],
        );
        assert!(receipt.passed);
        assert_eq!(receipt.schema_version, 1);
    }
}
