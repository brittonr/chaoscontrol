use chaoscontrol_protocol::admission::{token_for_descriptors, AssertionEvidenceIdentity};
use chaoscontrol_protocol::identity::{
    encode_lower_hex, AssertionDescriptor, AssertionKind, AssertionLogicalKey,
    ASSERTION_IDENTITY_VERSION,
};
use serde_json::{json, Value};
use std::path::Path;

const RAFT_ASSERTION_ID: u32 = 1;
const REDB_ASSERTION_ID: u32 = 2;
const SNAPSHOT_PATH: &str = "snapshots/fixture.snapshot.bin";

pub(crate) fn write_strict_replay_artifacts(evidence_dir: &Path, digest: &str) {
    let descriptors = [
        descriptor(RAFT_ASSERTION_ID, "raft-assertion"),
        descriptor(REDB_ASSERTION_ID, "redb-assertion"),
    ];
    let token = token_for_descriptors(&descriptors).expect("fixture catalog token");
    let identities = descriptors
        .iter()
        .map(|descriptor| {
            let fingerprint = descriptor.fingerprint().expect("fixture fingerprint");
            AssertionEvidenceIdentity {
                descriptor: descriptor.clone(),
                fingerprint,
                canonical_descriptor: descriptor
                    .canonical_bytes()
                    .expect("fixture canonical descriptor"),
                catalog_token: token,
            }
        })
        .collect::<Vec<_>>();

    write_json(
        &evidence_dir.join("assertions.json"),
        &assertion_summary(&identities),
    );
    write_bug(
        &evidence_dir.join("bug.json"),
        RAFT_ASSERTION_ID,
        &identities[0],
        digest,
    );
    write_bug(
        &evidence_dir.join("bug-redb.json"),
        REDB_ASSERTION_ID,
        &identities[1],
        digest,
    );
    write_verdict(
        &evidence_dir.join("verdict.json"),
        RAFT_ASSERTION_ID,
        &identities[0],
        digest,
    );
    write_verdict(
        &evidence_dir.join("verdict-redb.json"),
        REDB_ASSERTION_ID,
        &identities[1],
        digest,
    );
}

fn descriptor(assertion_id: u32, key: &str) -> AssertionDescriptor {
    AssertionDescriptor {
        identity_version: ASSERTION_IDENTITY_VERSION,
        namespace: "org.example.fixture".to_string(),
        logical_key: AssertionLogicalKey::Stable {
            key: key.to_string(),
        },
        kind: AssertionKind::Always,
        message: format!("message-{key}"),
        source_file: "src/main.rs".to_string(),
        source_line: 1,
        source_column: 1,
        guest: "guest".to_string(),
        category: "invariant".to_string(),
        compatibility_id: Some(assertion_id),
    }
}

fn assertion_summary(identities: &[AssertionEvidenceIdentity]) -> Value {
    let assertions = identities
        .iter()
        .map(|identity| {
            json!({
                "id": identity.descriptor.compatibility_id.expect("fixture alias"),
                "identity": {
                    "descriptor": identity.descriptor,
                    "fingerprint": identity.fingerprint,
                    "canonical_descriptor": encode_lower_hex(&identity.canonical_descriptor),
                    "catalog_tokens": [identity.catalog_token],
                },
                "message": identity.descriptor.message,
                "kind": identity.descriptor.kind,
                "guest": identity.descriptor.guest,
                "category": identity.descriptor.category,
                "hit_count": 1,
                "true_count": 1,
                "false_count": 0,
                "verdict": "passed",
            })
        })
        .collect::<Vec<_>>();
    json!({
        "schema": "chaoscontrol.assertion-summary.v2",
        "catalog_status": "accepted",
        "collision_safe_evidence": true,
        "assertions": assertions,
    })
}

fn write_bug(path: &Path, assertion_id: u32, identity: &AssertionEvidenceIdentity, digest: &str) {
    write_json(
        path,
        &json!({
            "bug_id": 0,
            "assertion_id": assertion_id,
            "assertion_identity": identity,
            "assertion_location": identity.descriptor.message,
            "schedule": {"faults": [{"time_ns": 1, "fault": {"NetworkHeal": null}, "label": null}]},
            "tick": 1,
            "replay_parent_depth": 1,
            "replay_parent_snapshot_ref": snapshot_ref(digest),
            "dedup_key": 1,
            "schedule_variant": null,
            "scenario_config": null,
            "scenario_summary": null,
        }),
    );
}

fn write_verdict(
    path: &Path,
    assertion_id: u32,
    identity: &AssertionEvidenceIdentity,
    digest: &str,
) {
    write_json(
        path,
        &json!({
            "schema_version": 2,
            "run_id": "fixture",
            "replay_class": "snapshot_backed_reproduced",
            "reproduced": true,
            "command": {"command": "fixture", "exit_status": 0},
            "diagnostic": "BUG REPRODUCED",
            "bug_path": path.file_name().expect("fixture verdict name").to_string_lossy().replace("verdict", "bug"),
            "bug_id": 0,
            "assertion_id": assertion_id,
            "assertion_identity": identity,
            "replay_parent_depth": 1,
            "snapshot": {"status": "valid", "present": true, "digest_verified": true, "reference": snapshot_ref(digest)},
            "artifact_hashes": [{"path": SNAPSHOT_PATH, "sha256": digest}],
        }),
    );
}

fn snapshot_ref(digest: &str) -> Value {
    json!({
        "store": "file-content-addressed",
        "digest": digest,
        "codec": "simulation-snapshot-cbor-zstd-v2",
        "schema_version": 2,
        "path": SNAPSHOT_PATH,
    })
}

fn write_json(path: &Path, value: &Value) {
    let bytes = serde_json::to_vec_pretty(value).expect("serialize strict fixture");
    std::fs::write(path, bytes).expect("write strict fixture");
}
