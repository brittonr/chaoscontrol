//! Determinism drift-gate receipt helpers.
//!
//! This module is intentionally KVM-free: the VM runner supplies fingerprints,
//! while this pure layer classifies mismatches and emits a machine-readable
//! receipt that CI/operator tooling can archive.

/// Stable fingerprint for one VM run.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VmFingerprint {
    /// Number of VM exits observed before the run stopped.
    pub exit_count: u64,
    /// Final virtual TSC value.
    pub virtual_tsc: u64,
    /// Serial output after documented nondeterministic lines are stripped.
    pub serial_stripped: String,
}

/// Stable fingerprint for one multi-VM controller run.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RunFingerprint {
    SingleVm(VmFingerprint),
    Controller(ControllerFingerprint),
}

/// One run inside a determinism case.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RunObservation {
    pub run_index: usize,
    pub fingerprint: RunFingerprint,
    /// Optional path to the dlog file or directory for this run.
    pub dlog_path: Option<String>,
}

/// Machine-readable mismatch details.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct MismatchDetail {
    pub run_index: usize,
    pub field: String,
    pub expected: String,
    pub actual: String,
    #[serde(default)]
    pub class: DivergenceClass,
}

/// Coarse first-divergence classes used to make drift receipts actionable.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct DlogDivergenceDetail {
    pub run_index: usize,
    pub class: DivergenceClass,
    pub summary: String,
}

/// Result for one deterministic configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

/// One required profile row in a bounded determinism matrix.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeterminismMatrixProfile {
    /// Stable row identifier, unique within one matrix receipt.
    pub row_id: String,
    /// Guest or workload identity, e.g. `raft`, `net`, or a smoke guest name.
    pub workload: String,
    /// Kernel artifact fingerprint used by this row.
    pub kernel_fingerprint: String,
    /// Initrd or guest artifact fingerprint used by this row.
    pub initrd_fingerprint: String,
    /// Named device profile covered by this row.
    pub device_profile: String,
    /// Named clock profile covered by this row.
    pub clock_profile: String,
    /// Controller or runner configuration label covered by this row.
    pub controller_profile: String,
    /// Current local product profile this row belongs to.
    pub local_product_profile: String,
    /// Number of local hypervisor workers represented by this row.
    pub worker_count: u32,
    /// Named local hypervisor/controller family for the row.
    pub hypervisor_profile: String,
}

/// One validated row in a bounded determinism matrix receipt.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeterminismMatrixRow {
    pub profile: DeterminismMatrixProfile,
    pub report: DeterminismCaseReport,
    /// Row status is explicit so unsupported or failing rows stay visible.
    #[serde(default = "default_matrix_row_status")]
    pub status: MatrixRowStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MatrixRowStatus {
    Passed,
    Failed,
    Unsupported,
}

fn default_matrix_row_status() -> MatrixRowStatus {
    MatrixRowStatus::Passed
}

impl MatrixRowStatus {
    fn matches_report(self, passed: bool) -> bool {
        matches!(
            (self, passed),
            (Self::Passed, true) | (Self::Failed, false) | (Self::Unsupported, false)
        )
    }
}

/// Top-level bounded device/profile matrix receipt.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeterminismMatrixReceipt {
    pub schema_version: u32,
    pub gate: String,
    pub matrix_id: String,
    pub rows: Vec<DeterminismMatrixRow>,
    pub passed: bool,
    /// Required anti-claim: rows not listed in this matrix remain unproven.
    pub unlisted_profiles_unproven: bool,
    /// Operator-facing bounded scope text. Must explicitly avoid arbitrary or
    /// universal guest/device determinism claims.
    pub scope: String,
}

impl DeterminismMatrixReceipt {
    pub fn new(matrix_id: impl Into<String>, rows: Vec<DeterminismMatrixRow>) -> Self {
        let passed = rows.iter().all(|row| row.report.passed);
        Self {
            schema_version: 1,
            gate: "vm-determinism-profile-matrix".to_string(),
            matrix_id: matrix_id.into(),
            rows,
            passed,
            unlisted_profiles_unproven: true,
            scope: "bounded device/profile matrix; unlisted profiles remain unproven; not arbitrary or universal guest/device determinism proof".to_string(),
        }
    }
}

/// Validate a matrix receipt against its required profile rows.
///
/// This pure check is intentionally stricter than serialization shape: it
/// rejects stale/missing rows, duplicate row IDs, weakened anti-claims, and rows
/// whose embedded comparison report does not match the aggregate pass/fail bit.
pub fn validate_determinism_matrix_receipt(
    receipt: &DeterminismMatrixReceipt,
    required_profiles: &[DeterminismMatrixProfile],
) -> Result<(), String> {
    if receipt.schema_version != 1 {
        return Err(format!(
            "matrix schema_version: expected 1, got {}",
            receipt.schema_version
        ));
    }
    if receipt.gate != "vm-determinism-profile-matrix" {
        return Err(format!(
            "matrix gate: expected vm-determinism-profile-matrix, got {:?}",
            receipt.gate
        ));
    }
    if receipt.rows.is_empty() {
        return Err("matrix rows: expected at least one row".to_string());
    }
    if !receipt.unlisted_profiles_unproven {
        return Err("matrix anti-claim: unlisted_profiles_unproven must be true".to_string());
    }
    let scope = receipt.scope.to_ascii_lowercase();
    if !(scope.contains("bounded")
        && scope.contains("unlisted")
        && scope.contains("unproven")
        && (scope.contains("not arbitrary") || scope.contains("not universal")))
    {
        return Err(
            "matrix scope: must state bounded scope, unlisted profiles remain unproven, and no arbitrary/universal determinism proof".to_string(),
        );
    }

    let mut seen = std::collections::BTreeSet::new();
    for row in &receipt.rows {
        if !seen.insert(row.profile.row_id.as_str()) {
            return Err(format!(
                "matrix row {:?}: duplicate row_id",
                row.profile.row_id
            ));
        }
        if row.profile.local_product_profile.is_empty() {
            return Err(format!(
                "matrix row {:?}: expected local_product_profile",
                row.profile.row_id
            ));
        }
        if row.profile.worker_count == 0 {
            return Err(format!(
                "matrix row {:?}: expected positive worker_count",
                row.profile.row_id
            ));
        }
        if row.profile.hypervisor_profile.is_empty() {
            return Err(format!(
                "matrix row {:?}: expected hypervisor_profile",
                row.profile.row_id
            ));
        }
        if row.status == MatrixRowStatus::Unsupported && row.report.mismatches.is_empty() {
            return Err(format!(
                "matrix row {:?}: unsupported rows must preserve bounded mismatch details",
                row.profile.row_id
            ));
        }
        if !row.status.matches_report(row.report.passed) {
            return Err(format!(
                "matrix row {:?}: status {:?} does not match report passed={}",
                row.profile.row_id, row.status, row.report.passed
            ));
        }
        if row.report.name.is_empty() {
            return Err(format!(
                "matrix row {:?}: expected non-empty report name",
                row.profile.row_id
            ));
        }
        if row.report.runs == 0 || row.report.observations.is_empty() {
            return Err(format!(
                "matrix row {:?}: expected non-empty observations",
                row.profile.row_id
            ));
        }
    }

    for required in required_profiles {
        if !receipt.rows.iter().any(|row| row.profile == *required) {
            return Err(format!(
                "matrix required profile {:?}: missing row",
                required.row_id
            ));
        }
    }

    let aggregate_passed = receipt.rows.iter().all(|row| row.report.passed);
    if receipt.passed != aggregate_passed {
        return Err(format!(
            "matrix passed: expected {aggregate_passed}, got {}",
            receipt.passed
        ));
    }

    Ok(())
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

    fn sample_profile(row_id: &str) -> DeterminismMatrixProfile {
        DeterminismMatrixProfile {
            row_id: row_id.to_string(),
            workload: "raft".to_string(),
            kernel_fingerprint: "sha256:kernel".to_string(),
            initrd_fingerprint: "sha256:initrd".to_string(),
            device_profile: "virtio-net-block".to_string(),
            clock_profile: "hide-tsc".to_string(),
            controller_profile: "single-vm".to_string(),
            local_product_profile: "single-machine-multi-hypervisor".to_string(),
            worker_count: 2,
            hypervisor_profile: "local-kvm-workers".to_string(),
        }
    }

    fn sample_single_vm_report(row_id: &str, actual_exit_count: u64) -> DeterminismCaseReport {
        compare_case(
            row_id,
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
                        exit_count: actual_exit_count,
                        virtual_tsc: 20,
                        serial_stripped: "ready".to_string(),
                    }),
                    dlog_path: None,
                },
            ],
        )
    }

    #[test]
    fn determinism_matrix_validates_required_profile_rows() {
        let profile = sample_profile("raft-hide-tsc");
        let row = DeterminismMatrixRow {
            profile: profile.clone(),
            report: sample_single_vm_report("raft-hide-tsc", 10),
            status: MatrixRowStatus::Passed,
        };
        let receipt = DeterminismMatrixReceipt::new("bounded-smoke", vec![row]);

        validate_determinism_matrix_receipt(&receipt, &[profile]).expect("matrix validates");
        assert!(receipt.passed);
        assert!(receipt.scope.contains("unlisted profiles remain unproven"));
    }

    #[test]
    fn determinism_matrix_rejects_failing_observation_with_mismatch_class() {
        let profile = sample_profile("raft-hide-tsc");
        let row = DeterminismMatrixRow {
            profile: profile.clone(),
            report: sample_single_vm_report("raft-hide-tsc", 11),
            status: MatrixRowStatus::Failed,
        };
        let receipt = DeterminismMatrixReceipt::new("bounded-smoke", vec![row]);

        validate_determinism_matrix_receipt(&receipt, &[profile]).expect("shape still valid");
        assert!(!receipt.passed);
        assert_eq!(
            receipt.rows[0].report.divergence_classes,
            vec![DivergenceClass::FingerprintCounters]
        );
    }

    #[test]
    fn determinism_matrix_rejects_missing_duplicate_and_weakened_anticlaims() {
        let profile = sample_profile("raft-hide-tsc");
        let row = DeterminismMatrixRow {
            profile: profile.clone(),
            report: sample_single_vm_report("raft-hide-tsc", 10),
            status: MatrixRowStatus::Passed,
        };

        let missing = DeterminismMatrixReceipt::new("bounded-smoke", vec![row.clone()]);
        let err = validate_determinism_matrix_receipt(&missing, &[sample_profile("net-hide-tsc")])
            .expect_err("missing row rejected");
        assert!(err.contains("missing row"));

        let duplicate = DeterminismMatrixReceipt::new("bounded-smoke", vec![row.clone(), row]);
        let err = validate_determinism_matrix_receipt(&duplicate, std::slice::from_ref(&profile))
            .expect_err("duplicate row rejected");
        assert!(err.contains("duplicate row_id"));

        let mut overclaim = DeterminismMatrixReceipt::new(
            "bounded-smoke",
            vec![DeterminismMatrixRow {
                profile: profile.clone(),
                report: sample_single_vm_report("raft-hide-tsc", 10),
                status: MatrixRowStatus::Passed,
            }],
        );
        overclaim.unlisted_profiles_unproven = false;
        overclaim.scope = "arbitrary guest/device determinism proof".to_string();
        let err = validate_determinism_matrix_receipt(&overclaim, &[profile])
            .expect_err("weakened anti-claim rejected");
        assert!(err.contains("unlisted_profiles_unproven") || err.contains("scope"));
    }

    #[test]
    fn determinism_matrix_requires_local_product_metadata_and_visible_unsupported_rows() {
        let mut profile = sample_profile("raft-hide-tsc");
        profile.worker_count = 0;
        let row = DeterminismMatrixRow {
            profile: profile.clone(),
            report: sample_single_vm_report("raft-hide-tsc", 10),
            status: MatrixRowStatus::Passed,
        };
        let err = validate_determinism_matrix_receipt(
            &DeterminismMatrixReceipt::new("bounded-smoke", vec![row]),
            &[profile],
        )
        .expect_err("missing worker count rejected");
        assert!(err.contains("worker_count"));

        let profile = sample_profile("raft-unsupported-hide-tsc");
        let unsupported = DeterminismMatrixRow {
            profile: profile.clone(),
            report: sample_single_vm_report("raft-unsupported-hide-tsc", 11),
            status: MatrixRowStatus::Unsupported,
        };
        let receipt = DeterminismMatrixReceipt::new("bounded-smoke", vec![unsupported]);
        validate_determinism_matrix_receipt(&receipt, &[profile])
            .expect("unsupported row remains visible with mismatch details");
        assert!(!receipt.passed);
    }
}
