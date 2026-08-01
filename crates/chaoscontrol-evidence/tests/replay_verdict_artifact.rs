use chaoscontrol_evidence::validate_snapshot_backed_replay_artifact;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

const FIXTURE_BUG: &str = "../../contracts/evidence/fixtures/valid/bug-report.identity.valid.json";
const SNAPSHOT_CODEC: &str = "simulation-snapshot-cbor-zstd-v2";
const SNAPSHOT_SCHEMA_VERSION: u64 = 2;
const REPLAY_VERDICT_SCHEMA_VERSION: u64 = 2;
const REPLAY_PARENT_DEPTH: u64 = 1;

fn sha256(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn write_json(path: &Path, value: &Value) -> Vec<u8> {
    let bytes = serde_json::to_vec(value).expect("serialize JSON fixture");
    std::fs::write(path, &bytes).expect("write JSON fixture");
    bytes
}

fn fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    let snapshots = root.join("snapshots");
    std::fs::create_dir(&snapshots).expect("create snapshot directory");
    let snapshot_bytes = b"bounded snapshot fixture";
    let snapshot_digest = sha256(snapshot_bytes);
    let snapshot_name = format!("{}.snapshot.bin", &snapshot_digest["sha256:".len()..]);
    let snapshot_relative = format!("snapshots/{snapshot_name}");
    let snapshot_path = root.join(&snapshot_relative);
    std::fs::write(&snapshot_path, snapshot_bytes).expect("write snapshot");
    let snapshot_ref = json!({
        "store": "file-content-addressed",
        "digest": snapshot_digest,
        "codec": SNAPSHOT_CODEC,
        "schema_version": SNAPSHOT_SCHEMA_VERSION,
        "path": snapshot_relative
    });

    let mut bug: Value =
        serde_json::from_str(&std::fs::read_to_string(FIXTURE_BUG).expect("read bug fixture"))
            .expect("parse bug fixture");
    bug["replay_parent_depth"] = json!(REPLAY_PARENT_DEPTH);
    bug["replay_parent_snapshot_ref"] = snapshot_ref.clone();
    let bug_path = root.join("bug.json");
    let bug_bytes = write_json(&bug_path, &bug);

    let verdict_path = root.join("verdict.json");
    let verdict = json!({
        "schema_version": REPLAY_VERDICT_SCHEMA_VERSION,
        "run_id": "replay-artifact-test",
        "replay_class": "snapshot_backed_reproduced",
        "reproduced": true,
        "command": {"command": "test reproduce", "exit_status": 0},
        "diagnostic": "fixture reproduced",
        "bug_path": bug_path.to_string_lossy(),
        "bug_id": bug["bug_id"],
        "assertion_id": bug["assertion_id"],
        "assertion_identity": bug["assertion_identity"],
        "replay_parent_depth": REPLAY_PARENT_DEPTH,
        "snapshot": {
            "status": "valid",
            "present": true,
            "digest_verified": true,
            "reference": snapshot_ref
        },
        "artifact_hashes": [
            {"path": bug_path.to_string_lossy(), "sha256": sha256(&bug_bytes)},
            {"path": snapshot_path.to_string_lossy(), "sha256": sha256(snapshot_bytes)}
        ]
    });
    write_json(&verdict_path, &verdict);
    (temp, verdict_path, bug_path)
}

#[test]
fn validates_exact_replay_verdict_artifact_join() {
    let (_temp, verdict, bug) = fixture();
    let summary = validate_snapshot_backed_replay_artifact(verdict, bug)
        .expect("exact replay artifact validates");

    assert_eq!(summary.replay_parent_depth, REPLAY_PARENT_DEPTH);
}

#[test]
fn rejects_snapshot_reference_substitution() {
    let (temp, verdict_path, bug_path) = fixture();
    let mut verdict: Value =
        serde_json::from_str(&std::fs::read_to_string(&verdict_path).expect("read verdict"))
            .expect("parse verdict");
    let substituted = temp.path().join("snapshots/substituted.snapshot.bin");
    let snapshot_bytes = b"bounded snapshot fixture";
    std::fs::write(&substituted, snapshot_bytes).expect("write substituted snapshot");
    verdict["snapshot"]["reference"]["path"] = json!("snapshots/substituted.snapshot.bin");
    verdict["artifact_hashes"][1]["path"] = json!(substituted.to_string_lossy());
    write_json(&verdict_path, &verdict);

    let error = validate_snapshot_backed_replay_artifact(verdict_path, bug_path)
        .expect_err("snapshot substitution must fail");
    assert!(error.message().contains("snapshot reference mismatch"));
}
