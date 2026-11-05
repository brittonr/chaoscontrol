//! Versioned JSON investigation requests over retained sessions.
//!
//! The requests are read-only: they admit a retained checkpoint, then locate,
//! bind a moment for, or inspect one recorded failure. No VM executes, no
//! artifact is written, and a response never promotes replay or correctness.
// r[impl chaoscontrol.agent_investigation.bug_event_reference]
// r[impl chaoscontrol.agent_investigation.locate_bugs]
// r[impl chaoscontrol.agent_investigation.inspect_bug]
// r[impl chaoscontrol.agent_investigation.moment_reference]

use crate::bounded_json::read_checkpoint;
use crate::checkpoint::{
    bind_moment, inspect_bug, inspect_snapshot, list_bugs_with_filter,
    plan_restore_with_snapshot_check, resolve_bug_selector, BugInspection, BugListEntry,
    BugListFilter, BugSelector, CheckpointBugExportError, MomentReference, RestorePlan,
    SnapshotInspection,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Request schema identifier.
pub const REQUEST_SCHEMA: &str = "chaoscontrol.investigation.request.v1";
/// Response schema identifier.
pub const RESPONSE_SCHEMA: &str = "chaoscontrol.investigation.response.v1";
/// Failure schema identifier.
pub const FAILURE_SCHEMA: &str = "chaoscontrol.investigation.failure.v1";
/// Largest accepted request document.
pub const MAX_REQUEST_BYTES: usize = 64 * 1024;
/// Largest accepted checkpoint path.
pub const MAX_PATH_BYTES: usize = 4096;
/// Largest number of located failures returned in one response.
pub const MAX_RESPONSE_BUGS: usize = 256;
/// Largest failure message retained in a response.
pub const MAX_MESSAGE_BYTES: usize = 512;

/// Non-claims retained on every successful response.
pub const NON_CLAIMS: [&str; 4] = [
    "A located or inspected failure is recorded evidence, not replay evidence.",
    "A moment reference or restore plan does not prove snapshot admissibility or reproducibility.",
    "Inspection reads stored reports and does not execute a VM or validate a candidate.",
    "No correctness, coverage-completeness, or release-eligibility claim follows.",
];

/// One read-only investigation operation.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum InvestigationOperation {
    /// List admitted failures, optionally filtered.
    Locate {
        /// Optional bounded filters.
        #[serde(default)]
        filter: LocateFilter,
    },
    /// Bind a stable moment for exactly one failure.
    Seek {
        /// Exact single-failure selector.
        selector: InvestigationSelector,
        /// Optional recorded tick that must equal the failure tick.
        #[serde(default)]
        tick: Option<u64>,
    },
    /// Inspect one failure, its parent snapshot summary, and its restore plan.
    Inspect {
        /// Exact single-failure selector.
        selector: InvestigationSelector,
        /// Verify the parent snapshot digest while planning.
        #[serde(default)]
        verify_snapshot: bool,
    },
}

/// Wire form of the bounded failure listing filters.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LocateFilter {
    /// Exact assertion id.
    #[serde(default)]
    pub assertion_id: Option<u64>,
    /// Exact stored dedup key.
    #[serde(default)]
    pub dedup_key: Option<u64>,
    /// Minimum recorded tick.
    #[serde(default)]
    pub min_tick: Option<u64>,
    /// Maximum recorded tick.
    #[serde(default)]
    pub max_tick: Option<u64>,
    /// Exact owner process instance.
    #[serde(default)]
    pub process_instance: Option<String>,
    /// Require an admitted assertion identity.
    #[serde(default)]
    pub require_assertion_identity: bool,
    /// Exact admitted assertion fingerprint (lower hex).
    #[serde(default)]
    pub assertion_fingerprint: Option<String>,
}

/// Exactly one failure selector.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "by", rename_all = "kebab-case", deny_unknown_fields)]
pub enum InvestigationSelector {
    /// Checkpoint bug position.
    Index {
        /// Zero-based checkpoint position.
        index: usize,
    },
    /// Stored dedup key.
    DedupKey {
        /// Exact dedup key.
        dedup_key: u64,
    },
    /// Admitted assertion fingerprint (lower hex).
    AssertionFingerprint {
        /// Exact fingerprint.
        fingerprint: String,
    },
}

/// One versioned investigation request.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InvestigationRequest {
    /// Request schema identifier.
    pub schema: String,
    /// Path to the retained checkpoint.
    pub checkpoint: String,
    /// Operation to perform.
    pub operation: InvestigationOperation,
}

/// Operation result variant.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "result", rename_all = "kebab-case")]
pub enum InvestigationResult {
    /// Listed failures in checkpoint order.
    Located {
        /// Retained failures.
        bugs: Vec<BugListEntry>,
        /// Matching failures omitted by the response bound.
        omitted: usize,
    },
    /// Bound moment for one failure.
    Seeked {
        /// Stable moment reference.
        moment: MomentReference,
    },
    /// Read-only failure inspection.
    Inspected {
        /// Stored failure report.
        bug: Box<BugInspection>,
        /// Parent snapshot summary, when the failure has one.
        snapshot: Option<Box<SnapshotInspection>>,
        /// Read-only restore plan.
        plan: Box<RestorePlan>,
    },
}

/// Successful response document.
#[derive(Debug, Clone, Serialize)]
pub struct InvestigationResponse {
    /// Response schema identifier.
    pub schema: &'static str,
    /// Request schema that produced this response.
    pub request_schema: String,
    /// Checkpoint content identity.
    pub checkpoint_digest: String,
    /// Operation result.
    #[serde(flatten)]
    pub result: InvestigationResult,
    /// Non-claims retained with the response.
    pub non_claims: Vec<&'static str>,
}

/// Typed failure document.
#[derive(Debug, Clone, Serialize)]
pub struct InvestigationFailure {
    /// Failure schema identifier.
    pub schema: &'static str,
    /// Stable failure class: `request`, `checkpoint`, `io`, or `selector`.
    pub kind: &'static str,
    /// Bounded message.
    pub message: String,
}

impl InvestigationFailure {
    /// Build a bounded failure document.
    pub fn new(kind: &'static str, message: String) -> Self {
        Self {
            schema: FAILURE_SCHEMA,
            kind,
            message: truncate_message(message),
        }
    }
}

fn truncate_message(message: String) -> String {
    if message.len() <= MAX_MESSAGE_BYTES {
        return message;
    }
    let mut end = MAX_MESSAGE_BYTES;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    message[..end].to_string()
}

/// Decode and admit one request without touching the filesystem.
pub fn decode_request(bytes: &[u8]) -> Result<InvestigationRequest, InvestigationFailure> {
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err(InvestigationFailure::new(
            "request",
            format!("request exceeds {MAX_REQUEST_BYTES} bytes"),
        ));
    }
    let request: InvestigationRequest = serde_json::from_slice(bytes).map_err(|error| {
        InvestigationFailure::new("request", format!("invalid request JSON: {error}"))
    })?;
    admit_request(&request)?;
    Ok(request)
}

/// Admit a decoded request's schema, path, and operation bounds.
pub fn admit_request(request: &InvestigationRequest) -> Result<(), InvestigationFailure> {
    if request.schema != REQUEST_SCHEMA {
        return Err(InvestigationFailure::new(
            "request",
            format!("unsupported request schema: {}", request.schema),
        ));
    }
    if request.checkpoint.is_empty()
        || request.checkpoint.len() > MAX_PATH_BYTES
        || request.checkpoint.contains('\0')
    {
        return Err(InvestigationFailure::new(
            "request",
            "checkpoint path is empty, too long, or contains a NUL".to_string(),
        ));
    }
    match &request.operation {
        InvestigationOperation::Locate { filter } => {
            if let (Some(min), Some(max)) = (filter.min_tick, filter.max_tick) {
                if min > max {
                    return Err(InvestigationFailure::new(
                        "request",
                        format!("min_tick {min} exceeds max_tick {max}"),
                    ));
                }
            }
            if let Some(fingerprint) = filter.assertion_fingerprint.as_deref() {
                chaoscontrol_protocol::identity::AssertionFingerprint::from_hex(fingerprint)
                    .map_err(|_| {
                        InvestigationFailure::new(
                            "request",
                            format!("invalid assertion fingerprint: {fingerprint}"),
                        )
                    })?;
            }
        }
        InvestigationOperation::Seek { selector, .. }
        | InvestigationOperation::Inspect { selector, .. } => admit_selector(selector)?,
    }
    Ok(())
}

fn admit_selector(selector: &InvestigationSelector) -> Result<(), InvestigationFailure> {
    if let InvestigationSelector::AssertionFingerprint { fingerprint } = selector {
        chaoscontrol_protocol::identity::AssertionFingerprint::from_hex(fingerprint).map_err(
            |_| {
                InvestigationFailure::new(
                    "request",
                    format!("invalid assertion fingerprint: {fingerprint}"),
                )
            },
        )?;
    }
    Ok(())
}

fn resolve_selector<P: AsRef<Path>>(
    checkpoint: P,
    selector: &InvestigationSelector,
) -> Result<BugSelector, CheckpointBugExportError> {
    match selector {
        InvestigationSelector::Index { index } => {
            resolve_bug_selector(checkpoint, Some(*index), None, None)
        }
        InvestigationSelector::DedupKey { dedup_key } => {
            resolve_bug_selector(checkpoint, None, Some(*dedup_key), None)
        }
        InvestigationSelector::AssertionFingerprint { fingerprint } => {
            resolve_bug_selector(checkpoint, None, None, Some(fingerprint))
        }
    }
}

fn locate_filter(filter: &LocateFilter) -> BugListFilter {
    BugListFilter {
        assertion_id: filter.assertion_id,
        dedup_key: filter.dedup_key,
        min_tick: filter.min_tick,
        max_tick: filter.max_tick,
        process_instance: filter.process_instance.clone(),
        require_assertion_identity: filter.require_assertion_identity,
        assertion_fingerprint: filter.assertion_fingerprint.clone(),
    }
}

fn classify(error: &CheckpointBugExportError) -> &'static str {
    match error {
        CheckpointBugExportError::Io { .. } => "io",
        CheckpointBugExportError::Checkpoint { .. }
        | CheckpointBugExportError::Json { .. }
        | CheckpointBugExportError::SnapshotStore { .. } => "checkpoint",
        _ => "selector",
    }
}

fn export_failure(error: CheckpointBugExportError) -> InvestigationFailure {
    InvestigationFailure::new(classify(&error), error.to_string())
}

/// Execute one admitted request against a retained checkpoint.
pub fn run_request(
    request: &InvestigationRequest,
) -> Result<InvestigationResponse, InvestigationFailure> {
    admit_request(request)?;
    let checkpoint = Path::new(&request.checkpoint);
    let bytes = read_checkpoint(checkpoint)
        .map_err(|error| InvestigationFailure::new("io", format!("read checkpoint: {error}")))?;
    let checkpoint_digest = format!("b3:{}", blake3::hash(bytes.as_bytes()).to_hex());
    let result = match &request.operation {
        InvestigationOperation::Locate { filter } => {
            let mut bugs =
                list_bugs_with_filter(checkpoint, locate_filter(filter)).map_err(export_failure)?;
            let omitted = bugs.len().saturating_sub(MAX_RESPONSE_BUGS);
            bugs.truncate(MAX_RESPONSE_BUGS);
            InvestigationResult::Located { bugs, omitted }
        }
        InvestigationOperation::Seek { selector, tick } => {
            let resolved = resolve_selector(checkpoint, selector).map_err(export_failure)?;
            let moment = bind_moment(checkpoint, resolved, *tick).map_err(export_failure)?;
            InvestigationResult::Seeked { moment }
        }
        InvestigationOperation::Inspect {
            selector,
            verify_snapshot,
        } => {
            let resolved = resolve_selector(checkpoint, selector).map_err(export_failure)?;
            let bug = inspect_bug(checkpoint, resolved).map_err(export_failure)?;
            let snapshot = if bug.bug.replay_parent_snapshot_ref.is_some() {
                Some(inspect_snapshot(checkpoint, resolved).map_err(export_failure)?)
            } else {
                None
            };
            let plan = plan_restore_with_snapshot_check(checkpoint, resolved, *verify_snapshot)
                .map_err(export_failure)?;
            InvestigationResult::Inspected {
                bug: Box::new(bug),
                snapshot: snapshot.map(Box::new),
                plan: Box::new(plan),
            }
        }
    };
    Ok(InvestigationResponse {
        schema: RESPONSE_SCHEMA,
        request_schema: request.schema.clone(),
        checkpoint_digest,
        result,
        non_claims: NON_CLAIMS.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::{
        save_checkpoint, CheckpointConfig, ExplorationCheckpoint, SerializableBug,
        SerializableSchedule,
    };
    use serde_json::json;

    const ASSERTION_ALIAS: u64 = 42;

    fn bug(bug_id: u64, tick: u64, dedup_key: Option<u64>) -> SerializableBug {
        SerializableBug {
            bug_id,
            assertion_id: ASSERTION_ALIAS,
            assertion_identity: Some(crate::test_support::assertion_identity(ASSERTION_ALIAS)),
            fallback_scope: None,
            process_instances: Default::default(),
            marker_context: None,
            assertion_location: "test.rs:1".into(),
            schedule: SerializableSchedule { faults: Vec::new() },
            tick,
            replay_parent_depth: 0,
            replay_parent_snapshot_ref: None,
            dedup_key,
            schedule_variant: None,
            scenario_config: None,
            scenario_summary: None,
            random_faults: false,
        }
    }

    fn checkpoint(bugs: Vec<SerializableBug>) -> ExplorationCheckpoint {
        ExplorationCheckpoint {
            config: CheckpointConfig {
                num_vms: 1,
                num_vcpus: 1,
                kernel_path: "/nonexistent-kernel".to_string(),
                initrd_path: None,
                extra_cmdline: None,
                seed: 1,
                branch_factor: 8,
                ticks_per_branch: 1000,
                max_rounds: 10,
                max_frontier: 50,
                quantum: 100,
                coverage_gpa: 0xE0000,
                disk_image_path: None,
                bootstrap_budget: 10_000,
                schedule_diversity: false,
                schedule_mutation_ratio: 0.0,
                rare_edge_threshold: None,
                rare_edge_weight: None,
                havoc_after_stale: None,
                havoc_mutations: None,
                scenario: None,
                frontier_policy: crate::explorer::FrontierPolicy::default(),
                random_faults: false,
            },
            global_coverage: vec![],
            bugs,
            assertion_report: Some(crate::test_support::assertion_report(ASSERTION_ALIAS, true)),
            rounds_completed: 1,
            total_branches_run: 8,
            total_edges: 10,
            seed: 1,
            round_history: None,
            seen_dedup_keys: None,
            scenario: None,
            scenario_summary: None,
            campaign_journal: None,
            release_eligible: true,
            assertion_checkpoint_selector: None,
        }
    }

    fn write_fixture(dir: &std::path::Path, bugs: Vec<SerializableBug>) -> String {
        let path = dir.join("checkpoint.json");
        save_checkpoint(&path, &checkpoint(bugs)).expect("checkpoint fixture saves");
        path.to_string_lossy().into_owned()
    }

    fn request(checkpoint: String, operation: InvestigationOperation) -> InvestigationRequest {
        InvestigationRequest {
            schema: REQUEST_SCHEMA.to_string(),
            checkpoint,
            operation,
        }
    }

    #[test]
    fn requests_are_admitted_and_rejected_with_bounded_messages() {
        let valid = json!({
            "schema": REQUEST_SCHEMA,
            "checkpoint": "checkpoint.json",
            "operation": {"kind": "locate", "filter": {"min_tick": 1, "max_tick": 2}}
        });
        let decoded = decode_request(valid.to_string().as_bytes()).expect("valid request");
        assert_eq!(
            decoded.operation,
            InvestigationOperation::Locate {
                filter: LocateFilter {
                    min_tick: Some(1),
                    max_tick: Some(2),
                    ..LocateFilter::default()
                }
            }
        );

        let mut unknown = valid.clone();
        unknown["unexpected"] = json!(true);
        assert!(decode_request(unknown.to_string().as_bytes()).is_err());

        let mut stale = valid.clone();
        stale["schema"] = json!("chaoscontrol.investigation.request.v0");
        assert!(decode_request(stale.to_string().as_bytes()).is_err());

        let mut bad_range = valid.clone();
        bad_range["operation"] =
            json!({"kind": "locate", "filter": {"min_tick": 5, "max_tick": 1}});
        assert!(decode_request(bad_range.to_string().as_bytes()).is_err());

        let mut bad_fingerprint = valid.clone();
        bad_fingerprint["operation"] = json!({"kind": "seek", "selector": {"by": "assertion-fingerprint", "fingerprint": "not-hex"}});
        assert!(decode_request(bad_fingerprint.to_string().as_bytes()).is_err());

        let mut empty_path = valid.clone();
        empty_path["checkpoint"] = json!("");
        assert!(decode_request(empty_path.to_string().as_bytes()).is_err());

        let oversized = vec![b' '; MAX_REQUEST_BYTES + 1];
        let failure = decode_request(&oversized).expect_err("oversized request rejects");
        assert_eq!(failure.kind, "request");
        assert!(failure.message.len() <= MAX_MESSAGE_BYTES);
    }

    #[test]
    fn locate_seek_and_inspect_read_one_retained_session() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = write_fixture(dir.path(), vec![bug(1, 100, Some(7)), bug(2, 200, Some(9))]);

        let located = run_request(&request(
            path.clone(),
            InvestigationOperation::Locate {
                filter: LocateFilter {
                    min_tick: Some(150),
                    ..LocateFilter::default()
                },
            },
        ))
        .expect("locate succeeds");
        match located.result {
            InvestigationResult::Located { bugs, omitted } => {
                assert_eq!(omitted, 0);
                assert_eq!(bugs.len(), 1);
                assert_eq!(bugs[0].bug_id, 2);
            }
            _ => panic!("locate result"),
        }
        assert!(located.checkpoint_digest.starts_with("b3:"));

        let seeked = run_request(&request(
            path.clone(),
            InvestigationOperation::Seek {
                selector: InvestigationSelector::DedupKey { dedup_key: 7 },
                tick: Some(100),
            },
        ))
        .expect("seek succeeds");
        match seeked.result {
            InvestigationResult::Seeked { moment } => {
                assert_eq!(moment.bug_id, 1);
                assert_eq!(moment.tick, 100);
                assert!(moment.branch_id.starts_with("b3:"));
            }
            _ => panic!("seek result"),
        }

        let inspected = run_request(&request(
            path.clone(),
            InvestigationOperation::Inspect {
                selector: InvestigationSelector::Index { index: 1 },
                verify_snapshot: false,
            },
        ))
        .expect("inspect succeeds");
        match inspected.result {
            InvestigationResult::Inspected {
                bug,
                snapshot,
                plan,
            } => {
                assert_eq!(bug.bug.bug_id, 2);
                assert!(snapshot.is_none());
                assert_eq!(plan.bug_id, 2);
                assert!(!plan.ready);
            }
            _ => panic!("inspect result"),
        }

        let missing = run_request(&request(
            path,
            InvestigationOperation::Inspect {
                selector: InvestigationSelector::DedupKey { dedup_key: 1234 },
                verify_snapshot: false,
            },
        ))
        .expect_err("unknown key rejects");
        assert_eq!(missing.kind, "selector");
    }

    #[test]
    fn located_results_are_bounded_and_accounted() {
        let dir = tempfile::tempdir().expect("temp dir");
        let bugs = (0..u64::try_from(MAX_RESPONSE_BUGS + 3).expect("bounded"))
            .map(|id| bug(id, id, Some(id)))
            .collect::<Vec<_>>();
        let path = write_fixture(dir.path(), bugs);
        let located = run_request(&request(
            path,
            InvestigationOperation::Locate {
                filter: LocateFilter::default(),
            },
        ))
        .expect("locate succeeds");
        match located.result {
            InvestigationResult::Located { bugs, omitted } => {
                assert_eq!(bugs.len(), MAX_RESPONSE_BUGS);
                assert_eq!(omitted, 3);
            }
            _ => panic!("locate result"),
        }
    }

    #[test]
    fn locate_filter_maps_every_field() {
        let keyed = locate_filter(&LocateFilter {
            assertion_id: Some(1),
            dedup_key: Some(2),
            min_tick: Some(3),
            max_tick: Some(4),
            process_instance: Some("p".into()),
            require_assertion_identity: true,
            assertion_fingerprint: None,
        });
        assert_eq!(keyed.assertion_id, Some(1));
        assert_eq!(keyed.dedup_key, Some(2));
        assert_eq!(keyed.min_tick, Some(3));
        assert_eq!(keyed.max_tick, Some(4));
        assert_eq!(keyed.process_instance.as_deref(), Some("p"));
        assert!(keyed.require_assertion_identity);
    }
}
