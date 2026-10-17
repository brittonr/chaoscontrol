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

fn bind_changed_bug(bug_path: &Path, bug: &Value, verdict_path: &Path, verdict: &mut Value) {
    let bytes = write_json(bug_path, bug);
    verdict["artifact_hashes"][0]["sha256"] = json!(sha256(&bytes));
    write_json(verdict_path, verdict);
}

#[test]
fn marker_join_rejects_scope_drift_missing_parent_and_corrupt_parent_bytes() {
    let (temp, verdict_path, bug_path) = fixture();
    let mut bug: Value = serde_json::from_slice(&std::fs::read(&bug_path).unwrap()).unwrap();
    let mut verdict: Value =
        serde_json::from_slice(&std::fs::read(&verdict_path).unwrap()).unwrap();
    let marker = json!({
        "marker_identity": format!("b3:{}", "a".repeat(64)),
        "owner": "guest-0/process/wal",
        "state_ref": format!("b3:{}", "b".repeat(64)),
        "logical_position_ref": "term:3/index:7",
        "run_id": 1,
        "observed_tick": 1
    });
    bug["marker_context"] = marker.clone();
    verdict["marker_context"] = marker.clone();
    bind_changed_bug(&bug_path, &bug, &verdict_path, &mut verdict);
    validate_snapshot_backed_replay_artifact(&verdict_path, &bug_path).unwrap();
    for (field, value) in [
        ("marker_identity", json!(format!("b3:{}", "c".repeat(64)))),
        ("owner", json!("guest-1")),
        ("state_ref", json!(format!("b3:{}", "c".repeat(64)))),
        ("logical_position_ref", json!("term:4")),
        ("run_id", json!(2)),
        ("observed_tick", json!(2)),
    ] {
        let mut drifted = verdict.clone();
        drifted["marker_context"][field] = value;
        write_json(&verdict_path, &drifted);
        assert!(
            validate_snapshot_backed_replay_artifact(&verdict_path, &bug_path).is_err(),
            "accepted {field} drift"
        );
    }
    let mut no_parent = bug.clone();
    no_parent["replay_parent_snapshot_ref"] = Value::Null;
    bind_changed_bug(&bug_path, &no_parent, &verdict_path, &mut verdict);
    assert!(validate_snapshot_backed_replay_artifact(&verdict_path, &bug_path).is_err());
    bind_changed_bug(&bug_path, &bug, &verdict_path, &mut verdict);
    let snapshot = bug["replay_parent_snapshot_ref"]["path"].as_str().unwrap();
    std::fs::write(temp.path().join(snapshot), b"different retained parent").unwrap();
    assert!(validate_snapshot_backed_replay_artifact(&verdict_path, &bug_path).is_err());
}

#[test]
fn artifact_join_binds_schedule_seed_strategy_quantum_and_policy_digest() {
    use chaoscontrol_replay_evidence_core::dto::{ReplayScheduleVariant, ReplayVerdict};
    use chaoscontrol_sim_core::scheduler::{ScheduleVariant, SchedulingStrategy};
    let (_temp, verdict_path, bug_path) = fixture();
    let mut bug: Value = serde_json::from_slice(&std::fs::read(&bug_path).unwrap()).unwrap();
    let mut typed_verdict: ReplayVerdict =
        serde_json::from_slice(&std::fs::read(&verdict_path).unwrap()).unwrap();
    let variant = ScheduleVariant {
        scheduler_seed: 73,
        strategy_override: Some(SchedulingStrategy::Randomized {
            min_quantum: 2,
            max_quantum: 9,
        }),
        quantum_override: Some(5),
    };
    bug["schedule_variant"] = serde_json::to_value(&variant).unwrap();
    typed_verdict.schedule_variant = Some(ReplayScheduleVariant {
        scheduler_seed: variant.scheduler_seed,
        strategy: serde_json::to_string(&variant.strategy_override).unwrap(),
        quantum_override: variant.quantum_override,
        policy_blake3: blake3::hash(&serde_json::to_vec(&variant).unwrap())
            .to_hex()
            .to_string(),
    });
    let mut verdict = serde_json::to_value(&typed_verdict).unwrap();
    bind_changed_bug(&bug_path, &bug, &verdict_path, &mut verdict);
    validate_snapshot_backed_replay_artifact(&verdict_path, &bug_path).unwrap();
    for (field, value) in [
        ("scheduler_seed", json!(74)),
        ("strategy", json!("null")),
        ("quantum_override", json!(6)),
        ("policy_blake3", json!("a".repeat(64))),
    ] {
        let mut drifted = verdict.clone();
        drifted["schedule_variant"][field] = value;
        write_json(&verdict_path, &drifted);
        chaoscontrol_evidence::validate_replay_verdict_with_options(
            &drifted,
            true,
            true,
            Path::new("/"),
        )
        .expect("drift remains a well-formed canonical runtime verdict");
        assert!(
            validate_snapshot_backed_replay_artifact(&verdict_path, &bug_path).is_err(),
            "accepted schedule {field} drift"
        );
    }
    for nested in [false, true] {
        let mut unknown = verdict.clone();
        let target = if nested {
            &mut unknown["schedule_variant"]
        } else {
            &mut unknown
        };
        target["invented_policy_field"] = json!(true);
        write_json(&verdict_path, &unknown);
        assert!(validate_snapshot_backed_replay_artifact(&verdict_path, &bug_path).is_err());
    }
}

#[test]
fn process_owner_join_rejects_scope_removal_and_substitution() {
    let (_temp, verdict_path, bug_path) = fixture();
    let mut bug: Value = serde_json::from_slice(&std::fs::read(&bug_path).unwrap()).unwrap();
    let mut verdict: Value =
        serde_json::from_slice(&std::fs::read(&verdict_path).unwrap()).unwrap();
    bug["process_instances"] = json!(["writer"]);
    verdict["process_instances"] = json!(["writer"]);
    bind_changed_bug(&bug_path, &bug, &verdict_path, &mut verdict);
    validate_snapshot_backed_replay_artifact(&verdict_path, &bug_path).unwrap();
    for owners in [json!([]), json!(["checkpoint"])] {
        let mut drifted = verdict.clone();
        drifted["process_instances"] = owners;
        write_json(&verdict_path, &drifted);
        let error = validate_snapshot_backed_replay_artifact(&verdict_path, &bug_path).unwrap_err();
        assert!(error.message().contains("process owner scope mismatch"));
    }
    verdict["process_instances"] = json!(["whole guest"]);
    write_json(&verdict_path, &verdict);
    assert!(validate_snapshot_backed_replay_artifact(&verdict_path, &bug_path).is_err());
}
