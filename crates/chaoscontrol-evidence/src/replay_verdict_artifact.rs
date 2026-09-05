use serde_json::Value;

use crate::{ensure, BugRecord, EvidenceError, EvidenceResult, ReplayVerdict};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayVerdictArtifactSummary {
    pub run_id: String,
    pub bug_id: u64,
    pub assertion_id: u64,
    pub replay_parent_depth: u64,
}

pub fn validate_snapshot_backed_replay_artifact(
    verdict_path: impl AsRef<std::path::Path>,
    expected_bug_path: impl AsRef<std::path::Path>,
) -> EvidenceResult<ReplayVerdictArtifactSummary> {
    let verdict_path = verdict_path.as_ref();
    let expected_bug_path = expected_bug_path.as_ref();
    ensure(
        verdict_path.is_absolute(),
        "replay verdict path must be absolute",
    )?;
    ensure(
        expected_bug_path.is_absolute(),
        "expected bug path must be absolute",
    )?;

    let filesystem_root = std::path::Path::new("/");
    let verdict_value: Value = crate::load_json(filesystem_root, verdict_path)?;
    crate::validate_replay_verdict_with_options(&verdict_value, true, true, filesystem_root)?;
    let verdict: ReplayVerdict = serde_json::from_value(verdict_value).map_err(|error| {
        EvidenceError::new(format!(
            "{}: invalid replay verdict: {error}",
            verdict_path.display()
        ))
    })?;
    verdict.validate_shape()?;
    ensure(
        std::path::Path::new(&verdict.bug_path) == expected_bug_path,
        "replay verdict bug path differs from the selected bug",
    )?;

    let bug_value: Value = crate::load_json(filesystem_root, expected_bug_path)?;
    crate::validate_bug_report_for_replay(&bug_value)?;
    let bug: BugRecord = serde_json::from_value(bug_value).map_err(|error| {
        EvidenceError::new(format!(
            "{}: invalid bug report: {error}",
            expected_bug_path.display()
        ))
    })?;
    ensure(
        bug.bug_id == verdict.bug_id,
        "replay verdict bug ID mismatch",
    )?;
    ensure(
        bug.assertion_id == verdict.assertion_id,
        "replay verdict assertion alias mismatch",
    )?;
    ensure(
        bug.assertion_identity == verdict.assertion_identity,
        "replay verdict assertion identity mismatch",
    )?;
    ensure(
        bug.replay_parent_depth == verdict.replay_parent_depth,
        "replay verdict parent depth mismatch",
    )?;
    ensure(
        bug.replay_parent_snapshot_ref.as_ref() == Some(&verdict.snapshot.reference),
        "replay verdict snapshot reference mismatch",
    )?;

    Ok(ReplayVerdictArtifactSummary {
        run_id: verdict.run_id,
        bug_id: verdict.bug_id,
        assertion_id: verdict.assertion_id,
        replay_parent_depth: verdict.replay_parent_depth,
    })
}
