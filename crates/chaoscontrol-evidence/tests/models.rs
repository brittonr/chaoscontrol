use chaoscontrol_evidence::{
    AcceptedWorkloadProofs, ReplayVerdict, SnapshotChunkManifest, REQUIRED_REPLAY_CLASS,
};

#[test]
fn parses_committed_accepted_workload_manifest() {
    let manifest = AcceptedWorkloadProofs::from_json_str(include_str!(
        "../../../dogfood-results/accepted-workload-proofs.json"
    ))
    .expect("manifest parses");

    manifest.validate_shape().expect("manifest shape is valid");
    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.required_replay_class, REQUIRED_REPLAY_CLASS);
    assert!(manifest.proofs.iter().any(|proof| proof.workload == "raft"));
    assert!(manifest.proofs.iter().any(|proof| proof.workload == "redb"));
}

#[test]
fn rejects_duplicate_workload_manifest() {
    let input = r#"{
      "schema_version": 1,
      "scope": "test",
      "anti_claims": [],
      "required_replay_class": "snapshot_backed_reproduced",
      "proofs": [
        {"workload":"raft","assertion_id":1,"evidence_dir":"e","summary":"s","bug":"b","verdict":"v","snapshot":"snapshots/a"},
        {"workload":"raft","assertion_id":2,"evidence_dir":"e","summary":"s","bug":"b","verdict":"v","snapshot":"snapshots/b"}
      ]
    }"#;

    let manifest = AcceptedWorkloadProofs::from_json_str(input).expect("manifest parses");
    let err = manifest
        .validate_shape()
        .expect_err("duplicate is rejected");
    assert!(err.message().contains("duplicate workload proof: raft"));
}

#[test]
fn parses_committed_replay_verdict_model() {
    let verdict: ReplayVerdict = serde_json::from_str(include_str!(
        "../../../dogfood-results/raft-accepted-verdict-dogfood-20260509T030143Z/replay-verdict-bug0.json"
    ))
    .expect("verdict parses");

    verdict.validate_shape().expect("verdict shape is valid");
    assert_eq!(
        verdict.snapshot.reference.codec,
        "simulation-snapshot-cbor-zstd-v2"
    );
    assert!(verdict.snapshot.reference.digest.starts_with("sha256:"));
}

#[test]
fn rejects_malformed_snapshot_ref() {
    let input = r#"{
      "schema_version": 1,
      "run_id": "run",
      "replay_class": "snapshot_backed_reproduced",
      "reproduced": true,
      "command": {"command": "cmd", "exit_status": 0},
      "diagnostic": "BUG REPRODUCED",
      "bug_path": "bug_0.json",
      "bug_id": 0,
      "assertion_id": 1,
      "replay_parent_depth": 1,
      "snapshot": {
        "status": "valid",
        "present": true,
        "digest_verified": true,
        "reference": {
          "store": "file-content-addressed",
          "digest": "md5:not-a-sha",
          "codec": "simulation-snapshot-cbor-zstd-v2",
          "schema_version": 2,
          "path": "snapshots/x.snapshot.bin"
        }
      },
      "artifact_hashes": []
    }"#;

    let verdict: ReplayVerdict = serde_json::from_str(input).expect("verdict parses");
    let err = verdict
        .validate_shape()
        .expect_err("bad digest is rejected");
    assert!(err.message().contains("snapshot digest is not sha256"));
}

#[test]
fn validates_snapshot_chunk_manifest_shape() {
    let manifest: SnapshotChunkManifest = serde_json::from_str(r#"{
      "schema_version": 1,
      "original_path": "abc.snapshot.bin",
      "original_size": 4,
      "original_sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
      "chunks": [
        {"path":"snapshots/abc.part-0000.bin", "size":4, "sha256":"abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"}
      ]
    }"#).expect("chunk manifest parses");

    manifest
        .validate_shape()
        .expect("chunk manifest shape is valid");
}

#[test]
fn rejects_unsafe_snapshot_chunk_path() {
    let manifest: SnapshotChunkManifest = serde_json::from_str(r#"{
      "schema_version": 1,
      "original_path": "abc.snapshot.bin",
      "original_size": 4,
      "original_sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
      "chunks": [
        {"path":"../abc.part-0000.bin", "size":4, "sha256":"abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"}
      ]
    }"#).expect("chunk manifest parses");

    let err = manifest
        .validate_shape()
        .expect_err("unsafe path is rejected");
    assert!(err.message().contains("chunk 0 path invalid"));
}
