//! Positive and negative fixtures for the shared replay/evidence core.
//!
//! Positive fixtures mirror current explorer verdict output and evidence
//! readiness accepted-proof records. Negative fixtures prove validation fails
//! closed with a diagnostic naming the invalid field or unsupported claim.

use chaoscontrol_protocol::admission::{
    token_for_descriptors, AssertionEvidenceIdentity, CatalogBuilder,
};
use chaoscontrol_protocol::identity::{
    AssertionDescriptor, AssertionKind, AssertionLogicalKey, ASSERTION_IDENTITY_VERSION,
};
use chaoscontrol_replay_evidence_core::claims::{
    find_forbidden_fragment, missing_required_fragments, FORBIDDEN_ASSERTION_OVERCLAIM_FRAGMENTS,
    REQUIRED_ASSERTION_ANTI_CLAIM_FRAGMENTS,
};
use chaoscontrol_replay_evidence_core::dto::{
    ArtifactHash, ReplayClass, ReplayCommandContext, ReplayParentSnapshotRef,
    ReplaySnapshotValidation, ReplayVerdict, REPLAY_VERDICT_SCHEMA_VERSION,
};
use chaoscontrol_replay_evidence_core::validate::{
    parse_replay_class, validate_accepted_proof, validate_public_verdict_paths,
    validate_verdict_consistency, verify_artifact_digest, CURRENT_SNAPSHOT_CODEC,
    CURRENT_SNAPSHOT_SCHEMA_VERSION, FILE_STORE_KIND,
};

const TEST_ALIAS: u64 = 1806003755;
const HEX_CHARS_IN_SHA256: usize = 64;
const HEX_CHARS_PER_BYTE: usize = 2;

fn test_identity(alias: u64) -> AssertionEvidenceIdentity {
    let compatibility_id = u32::try_from(alias).expect("fixture alias fits u32");
    let descriptor = AssertionDescriptor {
        identity_version: ASSERTION_IDENTITY_VERSION,
        namespace: "org.example.fixture".to_string(),
        logical_key: AssertionLogicalKey::Stable {
            key: format!("assertion-{alias}"),
        },
        kind: AssertionKind::Always,
        message: format!("assertion {alias}"),
        source_file: "src/fixture.rs".to_string(),
        source_line: 1,
        source_column: 1,
        guest: "fixture-guest".to_string(),
        category: "invariant".to_string(),
        compatibility_id: Some(compatibility_id),
    };
    let token = token_for_descriptors(std::slice::from_ref(&descriptor)).expect("catalog token");
    let mut builder = CatalogBuilder::begin(1).expect("catalog begins");
    builder.insert(descriptor).expect("descriptor inserts");
    let catalog = builder.complete(token).expect("catalog completes");
    let admitted = catalog
        .assertions
        .values()
        .next()
        .expect("admitted assertion");
    AssertionEvidenceIdentity::from_admitted(admitted, token).expect("evidence identity")
}

fn digest(byte: u8) -> String {
    let repeat = HEX_CHARS_IN_SHA256 / HEX_CHARS_PER_BYTE;
    format!("sha256:{}", format!("{byte:02x}").repeat(repeat))
}

fn snapshot_ref(byte: u8) -> ReplayParentSnapshotRef {
    ReplayParentSnapshotRef {
        store: FILE_STORE_KIND.to_string(),
        digest: digest(byte),
        codec: CURRENT_SNAPSHOT_CODEC.to_string(),
        schema_version: CURRENT_SNAPSHOT_SCHEMA_VERSION,
        path: format!("snapshots/{}.snapshot.bin", "ab".repeat(32)),
    }
}

fn bug_hash() -> ArtifactHash {
    ArtifactHash {
        path: "bug_2.json".to_string(),
        sha256: digest(0x42),
    }
}

fn snapshot_backed_verdict(
    class: ReplayClass,
    reproduced: bool,
    exit_status: i32,
) -> ReplayVerdict {
    ReplayVerdict {
        schema_version: REPLAY_VERDICT_SCHEMA_VERSION,
        run_id: "replay-fixture".to_string(),
        replay_class: class,
        reproduced,
        command: ReplayCommandContext {
            command: "chaoscontrol-explore reproduce --bug bug_2.json".to_string(),
            exit_status,
        },
        diagnostic: "fixture diagnostic".to_string(),
        bug_path: Some("bug_2.json".to_string()),
        bug_id: Some(2),
        assertion_id: Some(TEST_ALIAS),
        assertion_identity: Some(test_identity(TEST_ALIAS)),
        replay_parent_depth: Some(1),
        snapshot: ReplaySnapshotValidation::valid(snapshot_ref(0x77)),
        artifact_hashes: vec![
            bug_hash(),
            ArtifactHash {
                path: "snapshots/".to_string() + &"ab".repeat(32) + ".snapshot.bin",
                sha256: digest(0x77),
            },
        ],
    }
}

fn schedule_only_verdict() -> ReplayVerdict {
    ReplayVerdict {
        schema_version: REPLAY_VERDICT_SCHEMA_VERSION,
        run_id: "replay-fixture".to_string(),
        replay_class: ReplayClass::ScheduleOnlyReplayGap,
        reproduced: false,
        command: ReplayCommandContext {
            command: "chaoscontrol-explore reproduce --bug bug_0.json".to_string(),
            exit_status: 1,
        },
        diagnostic: "schedule-only bug: no snapshot context".to_string(),
        bug_path: Some("bug_0.json".to_string()),
        bug_id: Some(0),
        assertion_id: Some(TEST_ALIAS),
        assertion_identity: Some(test_identity(TEST_ALIAS)),
        replay_parent_depth: Some(0),
        snapshot: ReplaySnapshotValidation::not_required(),
        artifact_hashes: vec![ArtifactHash {
            path: "bug_0.json".to_string(),
            sha256: digest(0x11),
        }],
    }
}

// ---------------------------------------------------------------------
// Positive fixtures: current emitted verdict shapes remain accepted.
// ---------------------------------------------------------------------

#[test]
fn accepts_current_snapshot_backed_reproduced_verdict() {
    let verdict = snapshot_backed_verdict(ReplayClass::SnapshotBackedReproduced, true, 0);
    validate_verdict_consistency(&verdict).expect("consistent verdict");
    validate_accepted_proof(&verdict).expect("accepted proof");
}

#[test]
fn snapshot_backed_verdict_json_keeps_public_field_names() {
    let verdict = snapshot_backed_verdict(ReplayClass::SnapshotBackedReproduced, true, 0);
    let json = serde_json::to_value(&verdict).expect("verdict serializes");
    for field in [
        "schema_version",
        "run_id",
        "replay_class",
        "reproduced",
        "command",
        "diagnostic",
        "bug_path",
        "bug_id",
        "assertion_id",
        "assertion_identity",
        "replay_parent_depth",
        "snapshot",
        "artifact_hashes",
    ] {
        assert!(json.get(field).is_some(), "public field {field} lost");
    }
    assert_eq!(
        json.get("replay_class").and_then(|value| value.as_str()),
        Some("snapshot_backed_reproduced")
    );
    let round_tripped: ReplayVerdict =
        serde_json::from_value(json).expect("verdict JSON deserializes");
    validate_verdict_consistency(&round_tripped).expect("round-tripped verdict stays valid");
}

#[test]
fn accepts_schedule_only_gap_without_snapshot() {
    let verdict = schedule_only_verdict();
    validate_verdict_consistency(&verdict).expect("schedule-only gap is consistent");
    assert!(validate_accepted_proof(&verdict).is_err());
}

#[test]
fn accepts_schedule_only_gap_with_zero_depth_valid_snapshot() {
    let mut verdict = schedule_only_verdict();
    verdict.snapshot = ReplaySnapshotValidation::valid(snapshot_ref(0x55));
    validate_verdict_consistency(&verdict).expect("zero-depth gap with valid ref is consistent");
}

#[test]
fn accepts_no_bug_found_classification() {
    let verdict = ReplayVerdict::no_bug_found(
        "replay-fixture".to_string(),
        "chaoscontrol-explore reproduce --bug missing.json".to_string(),
        "bug file not found",
    );
    validate_verdict_consistency(&verdict).expect("no-bug verdict is consistent");
}

#[test]
fn accepts_missing_snapshot_ref_classification() {
    let mut verdict = schedule_only_verdict();
    verdict.replay_class = ReplayClass::MissingSnapshotRef;
    verdict.replay_parent_depth = Some(2);
    verdict.snapshot =
        ReplaySnapshotValidation::missing_ref("bug lacks replay_parent_snapshot_ref");
    validate_verdict_consistency(&verdict).expect("missing-ref verdict is consistent");
}

// ---------------------------------------------------------------------
// Negative fixtures: invalid records fail closed with named fields.
// ---------------------------------------------------------------------

#[test]
fn rejects_malformed_artifact_hash() {
    let mut verdict = schedule_only_verdict();
    verdict.artifact_hashes[0].sha256 = "sha256:zz".to_string();
    let error = validate_verdict_consistency(&verdict).expect_err("malformed hash rejected");
    assert!(error.message().contains("artifact-hash.sha256"));
}

#[test]
fn rejects_missing_snapshot_reference_for_snapshot_backed_class() {
    let mut verdict = snapshot_backed_verdict(ReplayClass::SnapshotBackedReproduced, true, 0);
    verdict.snapshot.reference = None;
    let error = validate_verdict_consistency(&verdict).expect_err("missing reference rejected");
    assert!(error.message().contains("snapshot.reference"));
}

#[test]
fn rejects_invalid_snapshot_digest() {
    let mut verdict = snapshot_backed_verdict(ReplayClass::SnapshotBackedReproduced, true, 0);
    verdict
        .snapshot
        .reference
        .as_mut()
        .expect("reference")
        .digest = "sha256:123".to_string();
    let error = validate_verdict_consistency(&verdict).expect_err("invalid digest rejected");
    assert!(error.message().contains("snapshot-ref.digest"));
}

#[test]
fn rejects_path_escaping_artifact_ref_at_the_public_boundary() {
    let mut verdict = schedule_only_verdict();
    verdict.artifact_hashes[0].path = "../escape/bug_0.json".to_string();
    verdict.bug_path = Some("../escape/bug_0.json".to_string());
    let error = validate_public_verdict_paths(&verdict).expect_err("path escape rejected");
    assert!(error.message().contains("bug_path"));
}

#[test]
fn rejects_absolute_artifact_ref_at_the_public_boundary() {
    let mut verdict = schedule_only_verdict();
    verdict.artifact_hashes[0].path = "/tmp/bug_0.json".to_string();
    verdict.bug_path = Some("/tmp/bug_0.json".to_string());
    // Local replay tooling may record absolute paths; consistency allows them.
    validate_verdict_consistency(&verdict).expect("absolute local paths stay consistent");
    let error =
        validate_public_verdict_paths(&verdict).expect_err("absolute path rejected publicly");
    assert!(error.message().contains("escapes the evidence root"));
}

#[test]
fn rejects_unsupported_replay_class_string() {
    let error =
        parse_replay_class("hosted_global_determinism").expect_err("unsupported class rejected");
    assert!(error.message().contains("replay_class"));
}

#[test]
fn rejects_stale_artifact_hash() {
    let recorded = bug_hash();
    let recomputed = digest(0x99);
    let error =
        verify_artifact_digest(&recorded, &recomputed).expect_err("stale artifact hash rejected");
    assert!(error.message().contains("stale artifact hash"));
}

#[test]
fn rejects_non_reproducing_snapshot_backed_verdict_as_accepted_proof() {
    let verdict = snapshot_backed_verdict(ReplayClass::SnapshotBackedNotReproduced, false, 1);
    validate_verdict_consistency(&verdict).expect("not-reproduced verdict is consistent");
    let error = validate_accepted_proof(&verdict).expect_err("accepted proof fails closed");
    assert!(error.message().contains("replay_class"));
}

#[test]
fn rejects_legacy_schema_verdict_as_accepted_proof() {
    let mut verdict = snapshot_backed_verdict(ReplayClass::SnapshotBackedReproduced, true, 0);
    verdict.schema_version = 1;
    validate_verdict_consistency(&verdict).expect("legacy verdict stays readable");
    let error = validate_accepted_proof(&verdict).expect_err("legacy verdict cannot promote");
    assert!(error.message().contains("schema_version"));
}

#[test]
fn rejects_exit_status_that_conflicts_with_replay_class() {
    let verdict = snapshot_backed_verdict(ReplayClass::SnapshotBackedReproduced, true, 1);
    let error = validate_verdict_consistency(&verdict).expect_err("forged exit status rejected");
    assert!(error.message().contains("exit_status"));
}

#[test]
fn rejects_null_assertion_identity_carrier() {
    let mut json = serde_json::to_value(schedule_only_verdict()).expect("verdict serializes");
    json["assertion_identity"] = serde_json::Value::Null;
    assert!(serde_json::from_value::<ReplayVerdict>(json).is_err());
}

#[test]
fn rejects_global_determinism_overclaim_wording() {
    let overclaim = "this gate shows assertion coverage proves replay for the whole hypervisor";
    assert!(find_forbidden_fragment(overclaim, &FORBIDDEN_ASSERTION_OVERCLAIM_FRAGMENTS).is_some());

    let bounded = "Local harness coverage is not snapshot replay evidence";
    let missing = missing_required_fragments(bounded, &REQUIRED_ASSERTION_ANTI_CLAIM_FRAGMENTS);
    assert!(!missing.is_empty());
    assert!(find_forbidden_fragment(bounded, &FORBIDDEN_ASSERTION_OVERCLAIM_FRAGMENTS).is_none());
}

#[test]
fn rejects_stale_snapshot_codec_for_accepted_proof() {
    let mut verdict = snapshot_backed_verdict(ReplayClass::SnapshotBackedReproduced, true, 0);
    verdict
        .snapshot
        .reference
        .as_mut()
        .expect("reference")
        .codec = "simulation-snapshot-bincode-zstd-v1".to_string();
    validate_verdict_consistency(&verdict).expect("legacy codec stays readable");
    let error = validate_accepted_proof(&verdict).expect_err("stale codec cannot promote");
    assert!(error.message().contains("CBOR v2"));
}
