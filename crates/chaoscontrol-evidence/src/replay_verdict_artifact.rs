use std::path::Path;

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
    verdict_path: impl AsRef<Path>,
    expected_bug_path: impl AsRef<Path>,
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

    let filesystem_root = Path::new("/");
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
        Path::new(&verdict.bug_path) == expected_bug_path,
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
    validate_carrier_bindings(&bug, &verdict)?;

    Ok(ReplayVerdictArtifactSummary {
        run_id: verdict.run_id,
        bug_id: verdict.bug_id,
        assertion_id: verdict.assertion_id,
        replay_parent_depth: verdict.replay_parent_depth,
    })
}

fn validate_carrier_bindings(bug: &BugRecord, verdict: &ReplayVerdict) -> EvidenceResult<()> {
    ensure(
        bug.process_instances == verdict.process_instances,
        "replay verdict process owner scope mismatch",
    )?;
    chaoscontrol_replay_evidence_core::validate::validate_process_instances(&bug.process_instances)
        .map_err(|error| EvidenceError::new(error.to_string()))?;
    ensure(
        bug.fallback_scope == verdict.fallback_scope,
        "replay verdict fallback scope mismatch",
    )?;
    if let Some(scope) = bug.fallback_scope.as_ref() {
        scope
            .validate_against(bug.require_assertion_identity()?)
            .map_err(|error| EvidenceError::new(format!("invalid fallback scope: {error:?}")))?;
    }
    ensure(
        bug.marker_context == verdict.marker_context,
        "replay verdict marker context mismatch",
    )?;
    if let Some(marker) = bug.marker_context.as_ref() {
        marker
            .validate()
            .map_err(|error| EvidenceError::new(format!("invalid marker context: {error:?}")))?;
        ensure(
            bug.replay_parent_depth > 0 && bug.replay_parent_snapshot_ref.is_some(),
            "marker context requires a retained parent snapshot",
        )?;
        ensure(
            bug.tick.is_some_and(|tick| marker.observed_tick <= tick),
            "marker context tick exceeds bug tick",
        )?;
    }
    let expected = bug
        .schedule_variant
        .as_ref()
        .map(|variant| -> EvidenceResult<_> {
            let bytes = serde_json::to_vec(variant)
                .map_err(|error| EvidenceError::new(error.to_string()))?;
            let strategy = serde_json::to_string(&variant.strategy_override)
                .map_err(|error| EvidenceError::new(error.to_string()))?;
            Ok(
                chaoscontrol_replay_evidence_core::dto::ReplayScheduleVariant {
                    scheduler_seed: variant.scheduler_seed,
                    strategy,
                    quantum_override: variant.quantum_override,
                    policy_blake3: blake3::hash(&bytes).to_hex().to_string(),
                },
            )
        })
        .transpose()?;
    ensure(
        expected == verdict.schedule_variant,
        "replay verdict schedule policy mismatch",
    )
}
