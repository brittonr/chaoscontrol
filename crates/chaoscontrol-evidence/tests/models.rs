use chaoscontrol_evidence::{
    check_replay_proof_coverage_doc, render_replay_proof_coverage,
    render_replay_proof_coverage_doc, validate_replay_proof_coverage, AcceptedWorkloadProofs,
    ReplayVerdict, SnapshotChunkManifest, SnapshotStorage, REQUIRED_REPLAY_CLASS,
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
fn validates_committed_replay_proof_coverage() {
    let lines = validate_replay_proof_coverage("../..").expect("coverage validates");

    assert_eq!(lines.len(), 4);
    assert!(lines
        .iter()
        .any(|line| line.workload == "raft" && line.snapshot_storage == SnapshotStorage::Chunks));
    assert!(lines
        .iter()
        .any(|line| line.workload == "redb" && line.snapshot_storage == SnapshotStorage::Raw));

    let rendered = render_replay_proof_coverage(&lines);
    assert!(rendered.starts_with("replay proof coverage ok:\n"));
    assert!(rendered.contains("raft: snapshot_backed_reproduced"));
    assert!(rendered.contains("snapshot=sha256:"));
}

#[test]
fn validates_committed_replay_proof_coverage_doc() {
    let rendered = render_replay_proof_coverage_doc("../..").expect("doc renders");
    assert_eq!(
        rendered,
        include_str!("../../../docs/replay-proof-coverage.md")
    );
    assert!(rendered.contains("dogfood-results/raft-accepted-verdict-dogfood-20260509T030143Z/"));
    check_replay_proof_coverage_doc("../..").expect("committed doc is fresh");
}

#[test]
fn rejects_stale_replay_proof_coverage_doc() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    std::fs::create_dir_all(root.join("docs")).expect("create docs");
    std::fs::write(root.join("docs/replay-proof-coverage.md"), "stale\n").expect("write stale doc");
    write_valid_minimal_coverage_fixture(root);

    let err = check_replay_proof_coverage_doc(root).expect_err("stale doc rejected");
    assert!(err
        .message()
        .contains("docs/replay-proof-coverage.md is stale"));
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

#[test]
fn rejects_tampered_snapshot_digest_in_full_coverage_validator() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    let evidence_dir = root.join("dogfood-results/fake-proof");
    let snapshots = evidence_dir.join("snapshots");
    std::fs::create_dir_all(&snapshots).expect("create fixture dirs");
    std::fs::write(snapshots.join("fixture.snapshot.bin"), b"fixture snapshot")
        .expect("write snapshot");

    std::fs::write(
        root.join("dogfood-results/accepted-workload-proofs.json"),
        r#"{
          "schema_version": 1,
          "scope": "test",
          "anti_claims": [],
          "required_replay_class": "snapshot_backed_reproduced",
          "proofs": [
            {"workload":"raft","assertion_id":1,"evidence_dir":"dogfood-results/fake-proof","summary":"summary.json","bug":"bug.json","verdict":"verdict.json","snapshot":"snapshots/fixture.snapshot.bin"},
            {"workload":"redb","assertion_id":2,"evidence_dir":"dogfood-results/fake-proof","summary":"summary-redb.json","bug":"bug-redb.json","verdict":"verdict-redb.json","snapshot":"snapshots/fixture.snapshot.bin"}
          ]
        }"#,
    )
    .expect("write manifest");
    write_summary(&evidence_dir.join("summary.json"), "raft", 1);
    write_summary(&evidence_dir.join("summary-redb.json"), "redb", 2);
    write_bug(&evidence_dir.join("bug.json"), 1);
    write_bug(&evidence_dir.join("bug-redb.json"), 2);
    write_verdict(
        &evidence_dir.join("verdict.json"),
        1,
        "sha256:0000000000000000000000000000000000000000000000000000000000000000",
    );
    write_verdict(
        &evidence_dir.join("verdict-redb.json"),
        2,
        "sha256:0000000000000000000000000000000000000000000000000000000000000000",
    );

    let err = validate_replay_proof_coverage(root).expect_err("tamper is rejected");
    assert!(err.message().contains("raft: snapshot digest mismatch"));
}

fn write_valid_minimal_coverage_fixture(root: &std::path::Path) {
    let evidence_dir = root.join("dogfood-results/fake-proof");
    let snapshots = evidence_dir.join("snapshots");
    std::fs::create_dir_all(&snapshots).expect("create fixture dirs");
    std::fs::write(snapshots.join("fixture.snapshot.bin"), b"fixture snapshot")
        .expect("write snapshot");
    std::fs::write(
        root.join("dogfood-results/accepted-workload-proofs.json"),
        r#"{
          "schema_version": 1,
          "scope": "test",
          "anti_claims": [],
          "required_replay_class": "snapshot_backed_reproduced",
          "proofs": [
            {"workload":"raft","assertion_id":1,"evidence_dir":"dogfood-results/fake-proof","summary":"summary.json","bug":"bug.json","verdict":"verdict.json","snapshot":"snapshots/fixture.snapshot.bin"},
            {"workload":"redb","assertion_id":2,"evidence_dir":"dogfood-results/fake-proof","summary":"summary-redb.json","bug":"bug-redb.json","verdict":"verdict-redb.json","snapshot":"snapshots/fixture.snapshot.bin"}
          ]
        }"#,
    )
    .expect("write manifest");
    write_summary(&evidence_dir.join("summary.json"), "raft", 1);
    write_summary(&evidence_dir.join("summary-redb.json"), "redb", 2);
    write_bug(&evidence_dir.join("bug.json"), 1);
    write_bug(&evidence_dir.join("bug-redb.json"), 2);
    let digest = "sha256:181b5fc5c39e672546f5611977eabee17a4ef4dc262fd1d74d7d07d250e2fd81";
    write_verdict(&evidence_dir.join("verdict.json"), 1, digest);
    write_verdict(&evidence_dir.join("verdict-redb.json"), 2, digest);
}

fn write_summary(path: &std::path::Path, workload: &str, assertion_id: u64) {
    std::fs::write(
        path,
        format!(
            r#"{{
              "workload": "{workload}",
              "seed": 1,
              "snapshot_probe_fail_after": 1,
              "run_exit_status": 1,
              "export_exit_status": 0,
              "reproduce_exit_status": 0,
              "bugs": [{{"file":"bug.json","assertion_id":{assertion_id},"replay_parent_depth":1,"has_snapshot_ref":true}}],
              "verdict": {{"path":"verdict.json","replay_class":"snapshot_backed_reproduced","reproduced":true,"replay_parent_depth":1,"snapshot_status":"valid"}},
              "accepted": true,
              "accepted_bug": "bug.json",
              "accepted_verdict": "verdict.json"
            }}"#
        ),
    )
    .expect("write summary");
}

fn write_bug(path: &std::path::Path, assertion_id: u64) {
    std::fs::write(
        path,
        format!(
            r#"{{
              "bug_id": 0,
              "assertion_id": {assertion_id},
              "assertion_location": "fixture",
              "tick": 1,
              "replay_parent_depth": 1,
              "replay_parent_snapshot_ref": {{"store":"file-content-addressed","digest":"sha256:fixture","codec":"simulation-snapshot-cbor-zstd-v2","schema_version":2,"path":"snapshots/fixture.snapshot.bin"}},
              "dedup_key": 1
            }}"#
        ),
    )
    .expect("write bug");
}

fn write_verdict(path: &std::path::Path, assertion_id: u64, digest: &str) {
    std::fs::write(
        path,
        format!(
            r#"{{
              "schema_version": 1,
              "run_id": "fixture",
              "replay_class": "snapshot_backed_reproduced",
              "reproduced": true,
              "command": {{"command":"fixture", "exit_status":0}},
              "diagnostic": "BUG REPRODUCED",
              "bug_path": "bug.json",
              "bug_id": 0,
              "assertion_id": {assertion_id},
              "replay_parent_depth": 1,
              "snapshot": {{"status":"valid","present":true,"digest_verified":true,"reference":{{"store":"file-content-addressed","digest":"{digest}","codec":"simulation-snapshot-cbor-zstd-v2","schema_version":2,"path":"snapshots/fixture.snapshot.bin"}}}},
              "artifact_hashes": []
            }}"#
        ),
    )
    .expect("write verdict");
}
