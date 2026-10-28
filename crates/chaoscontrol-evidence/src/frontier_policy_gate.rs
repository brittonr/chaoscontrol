//! Legacy-diagnostic frontier-policy outputs never satisfy release gates.
//!
//! After the Campaign cutover, exploration selects the Campaign-backed
//! frontier policy by default. The legacy in-process frontier survives only as
//! the explicitly selected `legacy-diagnostic` rollback. A diagnostic run
//! marks everything it writes:
//!
//! - `frontier-policy.json` ([`FRONTIER_POLICY_RECEIPT_FILE`]), a receipt
//!   carrying `release_eligible: false`, written before any VM work;
//! - `checkpoint.json` with `config.frontier_policy = "legacy-diagnostic"` and
//!   top-level `release_eligible: false`;
//! - `report.txt` with the [`REPORT_RELEASE_INELIGIBLE_LINE`] line.
//!
//! The dogfood receipt materializer, the accepted dogfood config gate, and the
//! KVM release matrix call this pure classifier and refuse such outputs with a
//! typed [`LegacyDiagnosticRefusal`]. The Nickel dogfood receipt and KVM
//! release profile contracts mirror these names; keep them in sync.

use std::fmt;
use std::path::Path;

use serde_json::Value;

use crate::{EvidenceError, EvidenceResult};

/// Key naming the frontier policy in checkpoints, run configs, and receipts.
pub const FRONTIER_POLICY_KEY: &str = "frontier_policy";
/// Key carrying the release eligibility of an exploration output.
pub const RELEASE_ELIGIBLE_KEY: &str = "release_eligible";
/// Name of the default Campaign-backed frontier policy (absent keys mean it).
pub const CAMPAIGN_FRONTIER_POLICY: &str = "campaign";
/// Name of the explicit legacy diagnostic rollback policy.
pub const LEGACY_DIAGNOSTIC_FRONTIER_POLICY: &str = "legacy-diagnostic";
/// Every frontier policy name, in stable order.
pub const FRONTIER_POLICIES: [&str; 2] =
    [CAMPAIGN_FRONTIER_POLICY, LEGACY_DIAGNOSTIC_FRONTIER_POLICY];
/// CLI flag that selects the frontier policy.
pub const FRONTIER_POLICY_FLAG: &str = "--frontier-policy";

/// Receipt written under the output directory of every legacy-diagnostic run.
pub const FRONTIER_POLICY_RECEIPT_FILE: &str = "frontier-policy.json";
/// Schema of [`FRONTIER_POLICY_RECEIPT_FILE`].
pub const FRONTIER_POLICY_RECEIPT_SCHEMA: &str = "chaoscontrol.frontier-policy-receipt.v1";
/// Checkpoint written under an exploration output directory.
pub const CHECKPOINT_FILE: &str = "checkpoint.json";
/// Human report written under an exploration output directory.
pub const REPORT_FILE: &str = "report.txt";
/// Dogfood run config materialized next to a checkpoint.
pub const RUN_CONFIG_FILE: &str = "run-config.json";
/// Report line that marks a release-ineligible exploration output.
pub const REPORT_RELEASE_INELIGIBLE_LINE: &str =
    "Release eligible:       no (legacy-diagnostic frontier policy; diagnostic rollback output)";

/// Stable prefix of every refusal message; gates and fixture runners match on
/// it to prove the refusal came from this boundary.
pub const LEGACY_DIAGNOSTIC_REFUSAL: &str = "legacy-diagnostic-frontier-policy";

/// Largest exploration JSON or report file a gate reads.
const MAX_OUTPUT_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// Release and dogfood gates that refuse legacy-diagnostic outputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReleaseGate {
    /// `materialize-dogfood-receipt` and the flat run-config/receipt contracts.
    DogfoodReceipt,
    /// `check-accepted-dogfood-config`.
    AcceptedDogfoodConfig,
    /// The required KVM release matrix and its receipts.
    KvmReleaseMatrix,
}

impl ReleaseGate {
    pub const ALL: [Self; 3] = [
        Self::DogfoodReceipt,
        Self::AcceptedDogfoodConfig,
        Self::KvmReleaseMatrix,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DogfoodReceipt => "dogfood receipt",
            Self::AcceptedDogfoodConfig => "accepted dogfood config",
            Self::KvmReleaseMatrix => "KVM release matrix",
        }
    }
}

impl fmt::Display for ReleaseGate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Which mark identified the legacy-diagnostic output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LegacyDiagnosticMarker {
    /// A `frontier_policy` value of `legacy-diagnostic`.
    PolicySelection,
    /// A `release_eligible: false` value.
    ReleaseIneligible,
    /// A `frontier-policy.json` diagnostic receipt.
    PolicyReceipt,
    /// The release-ineligible `report.txt` line.
    ReportLine,
    /// A command that selects `--frontier-policy legacy-diagnostic`.
    CommandSelection,
}

impl LegacyDiagnosticMarker {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PolicySelection => "frontier_policy = legacy-diagnostic",
            Self::ReleaseIneligible => "release_eligible = false",
            Self::PolicyReceipt => "frontier-policy.json diagnostic receipt",
            Self::ReportLine => "release-ineligible report line",
            Self::CommandSelection => "--frontier-policy legacy-diagnostic argument",
        }
    }
}

impl fmt::Display for LegacyDiagnosticMarker {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Typed refusal: a legacy-diagnostic output was offered to a release gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyDiagnosticRefusal {
    pub gate: ReleaseGate,
    pub marker: LegacyDiagnosticMarker,
    pub location: String,
}

impl fmt::Display for LegacyDiagnosticRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{LEGACY_DIAGNOSTIC_REFUSAL}: {}: {} marks a legacy-diagnostic rollback output; it is not release eligible and cannot satisfy the {}",
            self.location, self.marker, self.gate
        )
    }
}

impl std::error::Error for LegacyDiagnosticRefusal {}

impl From<LegacyDiagnosticRefusal> for EvidenceError {
    fn from(refusal: LegacyDiagnosticRefusal) -> Self {
        Self::new(refusal.to_string())
    }
}

fn refusal(
    gate: ReleaseGate,
    marker: LegacyDiagnosticMarker,
    location: impl Into<String>,
) -> LegacyDiagnosticRefusal {
    LegacyDiagnosticRefusal {
        gate,
        marker,
        location: location.into(),
    }
}

// r[impl chaoscontrol.campaign_policy.rollback]
/// Refuse a `frontier_policy` value naming the legacy diagnostic policy.
/// Other values pass; shape validators own unknown names.
pub fn admit_policy_value(
    gate: ReleaseGate,
    location: &str,
    value: &Value,
) -> Result<(), LegacyDiagnosticRefusal> {
    if value.as_str() == Some(LEGACY_DIAGNOSTIC_FRONTIER_POLICY) {
        return Err(refusal(
            gate,
            LegacyDiagnosticMarker::PolicySelection,
            location,
        ));
    }
    Ok(())
}

// r[impl chaoscontrol.campaign_policy.rollback]
/// Refuse an explicit `release_eligible: false`; absence means eligible.
pub fn admit_release_eligibility(
    gate: ReleaseGate,
    location: &str,
    value: Option<&Value>,
) -> Result<(), LegacyDiagnosticRefusal> {
    if value == Some(&Value::Bool(false)) {
        return Err(refusal(
            gate,
            LegacyDiagnosticMarker::ReleaseIneligible,
            location,
        ));
    }
    Ok(())
}

// r[impl chaoscontrol.campaign_policy.rollback]
/// Refuse a checkpoint recorded under, or carrying rounds from, the legacy
/// diagnostic policy.
pub fn admit_checkpoint(
    gate: ReleaseGate,
    location: &str,
    checkpoint: &Value,
) -> Result<(), LegacyDiagnosticRefusal> {
    if let Some(policy) = checkpoint
        .get("config")
        .and_then(|config| config.get(FRONTIER_POLICY_KEY))
    {
        admit_policy_value(
            gate,
            &format!("{location}.config.{FRONTIER_POLICY_KEY}"),
            policy,
        )?;
    }
    admit_release_eligibility(
        gate,
        &format!("{location}.{RELEASE_ELIGIBLE_KEY}"),
        checkpoint.get(RELEASE_ELIGIBLE_KEY),
    )
}

// r[impl chaoscontrol.campaign_policy.rollback]
/// Refuse an artifact path naming the diagnostic `frontier-policy.json` receipt.
///
/// Components are split on `/` and `\` and compared ASCII case-insensitively
/// so case-folding filesystems cannot smuggle the receipt through.
pub fn admit_artifact_path(
    gate: ReleaseGate,
    location: &str,
    path: &str,
) -> Result<(), LegacyDiagnosticRefusal> {
    let names_receipt = path
        .split(['/', '\\'])
        .next_back()
        .is_some_and(|name| name.eq_ignore_ascii_case(FRONTIER_POLICY_RECEIPT_FILE));
    if names_receipt {
        return Err(refusal(
            gate,
            LegacyDiagnosticMarker::PolicyReceipt,
            location,
        ));
    }
    Ok(())
}

// r[impl chaoscontrol.campaign_policy.rollback]
/// Refuse a human report carrying the release-ineligible line.
pub fn admit_report_text(
    gate: ReleaseGate,
    location: &str,
    text: &str,
) -> Result<(), LegacyDiagnosticRefusal> {
    if text
        .lines()
        .any(|line| line.trim_end() == REPORT_RELEASE_INELIGIBLE_LINE)
    {
        return Err(refusal(gate, LegacyDiagnosticMarker::ReportLine, location));
    }
    Ok(())
}

// r[impl chaoscontrol.campaign_policy.rollback]
/// Refuse a command line that selects the legacy diagnostic policy, either as
/// `--frontier-policy legacy-diagnostic` or `--frontier-policy=legacy-diagnostic`.
pub fn admit_command_args(
    gate: ReleaseGate,
    location: &str,
    args: &[String],
) -> Result<(), LegacyDiagnosticRefusal> {
    let joined_selection = format!("{FRONTIER_POLICY_FLAG}={LEGACY_DIAGNOSTIC_FRONTIER_POLICY}");
    let separate_selection = args.windows(2).any(|pair| {
        pair[0] == FRONTIER_POLICY_FLAG && pair[1] == LEGACY_DIAGNOSTIC_FRONTIER_POLICY
    });
    if separate_selection || args.contains(&joined_selection) {
        return Err(refusal(
            gate,
            LegacyDiagnosticMarker::CommandSelection,
            location,
        ));
    }
    Ok(())
}

/// Refuse an exploration output directory that holds any legacy-diagnostic
/// mark: the `frontier-policy.json` receipt, a diagnostic `checkpoint.json`,
/// a release-ineligible `report.txt`, or a diagnostic `run-config.json`.
///
/// Files that do not exist are skipped; shape validation belongs to the caller.
pub fn admit_output_directory(gate: ReleaseGate, directory: &Path) -> EvidenceResult<()> {
    if directory.as_os_str().is_empty() {
        return Err(EvidenceError::new(
            "output directory must not be empty".to_string(),
        ));
    }
    match std::fs::symlink_metadata(directory) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(EvidenceError::new(format!(
                "output directory must not be a symlink: {}",
                directory.display()
            )));
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err(EvidenceError::new(format!(
                "output path is not a directory: {}",
                directory.display()
            )));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let receipt = directory.join(FRONTIER_POLICY_RECEIPT_FILE);
    if std::fs::symlink_metadata(&receipt).is_ok() {
        return Err(refusal(
            gate,
            LegacyDiagnosticMarker::PolicyReceipt,
            receipt.display().to_string(),
        )
        .into());
    }
    let checkpoint = directory.join(CHECKPOINT_FILE);
    if checkpoint.is_file() {
        admit_checkpoint(
            gate,
            &checkpoint.display().to_string(),
            &load_json(&checkpoint)?,
        )?;
    }
    let report = directory.join(REPORT_FILE);
    if report.is_file() {
        let text = crate::bounded_file::read_bounded_regular_file(&report, MAX_OUTPUT_FILE_BYTES)?;
        admit_report_text(gate, &report.display().to_string(), &text)?;
    }
    let run_config = directory.join(RUN_CONFIG_FILE);
    if run_config.is_file() {
        if let Some(policy) = load_json(&run_config)?.get(FRONTIER_POLICY_KEY) {
            admit_policy_value(
                gate,
                &format!("{}.{FRONTIER_POLICY_KEY}", run_config.display()),
                policy,
            )?;
        }
    }
    Ok(())
}

fn load_json(path: &Path) -> EvidenceResult<Value> {
    let text = crate::bounded_file::read_bounded_regular_file(path, MAX_OUTPUT_FILE_BYTES)?;
    serde_json::from_str(&text)
        .map_err(|error| EvidenceError::new(format!("{}: invalid JSON: {error}", path.display())))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const LOCATION: &str = "fixture";

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn assert_refused(
        result: Result<(), LegacyDiagnosticRefusal>,
        gate: ReleaseGate,
        marker: LegacyDiagnosticMarker,
    ) {
        let refusal = result.expect_err("legacy-diagnostic mark is refused");
        assert_eq!((refusal.gate, refusal.marker), (gate, marker));
        let message = refusal.to_string();
        assert!(message.starts_with(LEGACY_DIAGNOSTIC_REFUSAL), "{message}");
        assert!(message.contains(gate.as_str()), "{message}");
    }

    #[test]
    fn campaign_and_absent_policy_values_are_admitted() {
        for gate in ReleaseGate::ALL {
            admit_policy_value(gate, LOCATION, &json!(CAMPAIGN_FRONTIER_POLICY)).unwrap();
            admit_release_eligibility(gate, LOCATION, None).unwrap();
            admit_release_eligibility(gate, LOCATION, Some(&json!(true))).unwrap();
            admit_checkpoint(gate, LOCATION, &json!({"config": {"seed": 1}})).unwrap();
            admit_checkpoint(
                gate,
                LOCATION,
                &json!({"config": {FRONTIER_POLICY_KEY: CAMPAIGN_FRONTIER_POLICY}}),
            )
            .unwrap();
        }
    }

    #[test]
    fn legacy_diagnostic_policy_and_ineligibility_are_refused_at_every_gate() {
        for gate in ReleaseGate::ALL {
            assert_refused(
                admit_policy_value(gate, LOCATION, &json!(LEGACY_DIAGNOSTIC_FRONTIER_POLICY)),
                gate,
                LegacyDiagnosticMarker::PolicySelection,
            );
            assert_refused(
                admit_release_eligibility(gate, LOCATION, Some(&json!(false))),
                gate,
                LegacyDiagnosticMarker::ReleaseIneligible,
            );
            assert_refused(
                admit_checkpoint(
                    gate,
                    LOCATION,
                    &json!({"config": {FRONTIER_POLICY_KEY: LEGACY_DIAGNOSTIC_FRONTIER_POLICY}}),
                ),
                gate,
                LegacyDiagnosticMarker::PolicySelection,
            );
            // A Campaign resume of a diagnostic checkpoint stays ineligible.
            assert_refused(
                admit_checkpoint(
                    gate,
                    LOCATION,
                    &json!({"config": {}, RELEASE_ELIGIBLE_KEY: false}),
                ),
                gate,
                LegacyDiagnosticMarker::ReleaseIneligible,
            );
        }
    }

    #[test]
    fn receipt_paths_are_refused_case_insensitively_and_other_paths_pass() {
        let gate = ReleaseGate::KvmReleaseMatrix;
        for path in [
            "frontier-policy.json",
            "fresh/attempts/attempt-01/frontier-policy.json",
            "fresh\\FRONTIER-POLICY.JSON",
        ] {
            assert_refused(
                admit_artifact_path(gate, LOCATION, path),
                gate,
                LegacyDiagnosticMarker::PolicyReceipt,
            );
        }
        for path in [
            "stdout.log",
            "frontier-policy.json/report.txt",
            "fresh/frontier-policy.json.bak",
            "fresh/frontier-trace.jsonl",
        ] {
            admit_artifact_path(gate, LOCATION, path).unwrap();
        }
    }

    #[test]
    fn report_line_is_refused_only_when_present_as_a_line() {
        let gate = ReleaseGate::DogfoodReceipt;
        let marked = format!(
            "Frontier policy:        legacy-diagnostic\n{REPORT_RELEASE_INELIGIBLE_LINE}\n"
        );
        assert_refused(
            admit_report_text(gate, LOCATION, &marked),
            gate,
            LegacyDiagnosticMarker::ReportLine,
        );
        admit_report_text(gate, LOCATION, "Frontier policy:        campaign\n").unwrap();
    }

    #[test]
    fn command_selection_is_refused_in_both_spellings() {
        let gate = ReleaseGate::KvmReleaseMatrix;
        for command in [
            args(&[
                "run",
                FRONTIER_POLICY_FLAG,
                LEGACY_DIAGNOSTIC_FRONTIER_POLICY,
            ]),
            args(&["run", "--frontier-policy=legacy-diagnostic"]),
        ] {
            assert_refused(
                admit_command_args(gate, LOCATION, &command),
                gate,
                LegacyDiagnosticMarker::CommandSelection,
            );
        }
        for command in [
            args(&["run", FRONTIER_POLICY_FLAG, CAMPAIGN_FRONTIER_POLICY]),
            args(&["run", "--output", LEGACY_DIAGNOSTIC_FRONTIER_POLICY]),
            args(&["run"]),
        ] {
            admit_command_args(gate, LOCATION, &command).unwrap();
        }
    }

    #[test]
    fn output_directory_rejects_an_empty_path() {
        assert!(admit_output_directory(ReleaseGate::DogfoodReceipt, Path::new("")).is_err());
    }

    #[test]
    fn output_directory_rejects_symlinks_and_files() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target");
        std::fs::create_dir(&target).unwrap();
        let link = root.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(admit_output_directory(ReleaseGate::DogfoodReceipt, &link).is_err());
        let file = root.path().join("file");
        std::fs::write(&file, b"file").unwrap();
        assert!(admit_output_directory(ReleaseGate::DogfoodReceipt, &file).is_err());
    }

    #[test]
    fn output_directory_refuses_each_diagnostic_mark() {
        let gate = ReleaseGate::DogfoodReceipt;
        let clean = tempfile::tempdir().unwrap();
        std::fs::write(
            clean.path().join(CHECKPOINT_FILE),
            json!({"config": {"seed": 1}}).to_string(),
        )
        .unwrap();
        std::fs::write(
            clean.path().join(REPORT_FILE),
            "Frontier policy:        campaign\n",
        )
        .unwrap();
        admit_output_directory(gate, clean.path()).expect("Campaign output is admitted");

        let cases: [(&str, String); 4] = [
            (
                FRONTIER_POLICY_RECEIPT_FILE,
                json!({"schema": FRONTIER_POLICY_RECEIPT_SCHEMA}).to_string(),
            ),
            (
                CHECKPOINT_FILE,
                json!({"config": {FRONTIER_POLICY_KEY: LEGACY_DIAGNOSTIC_FRONTIER_POLICY}, RELEASE_ELIGIBLE_KEY: false}).to_string(),
            ),
            (REPORT_FILE, format!("{REPORT_RELEASE_INELIGIBLE_LINE}\n")),
            (
                RUN_CONFIG_FILE,
                json!({FRONTIER_POLICY_KEY: LEGACY_DIAGNOSTIC_FRONTIER_POLICY}).to_string(),
            ),
        ];
        for (file, contents) in cases {
            let directory = tempfile::tempdir().unwrap();
            std::fs::write(directory.path().join(file), contents).unwrap();
            let error = admit_output_directory(gate, directory.path())
                .expect_err("diagnostic output is refused");
            assert!(
                error.message().starts_with(LEGACY_DIAGNOSTIC_REFUSAL),
                "{file}: {}",
                error.message()
            );
        }
    }
}
