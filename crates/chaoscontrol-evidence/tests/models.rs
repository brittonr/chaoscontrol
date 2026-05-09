use chaoscontrol_evidence::{
    check_assertion_readiness_promotion, check_assertion_readiness_status,
    check_dogfood_artifact_sizes, check_replay_proof_coverage_doc, check_replay_readiness_status,
    materialize_snapshot_chunks, render_assertion_readiness_status, render_replay_proof_coverage,
    render_replay_proof_coverage_doc, render_replay_readiness_dashboard,
    render_replay_readiness_readme_status_block, render_replay_readiness_status,
    run_assertion_readiness_promotion_selftest, run_dogfood_guards_selftest,
    run_materialize_snapshot_chunks_selftest, run_readiness_surface_drift_selftest,
    sample_replay_readiness_receipt, summarize_replay_readiness_receipt, summarize_sdk_local_jsonl,
    validate_accepted_dogfood_config, validate_assertion_readiness_promotion,
    validate_gate_metadata, validate_replay_proof_coverage, write_snapshot_chunk_fixture,
    AcceptedWorkloadProofs, ReplayVerdict, SnapshotChunkManifest, SnapshotStorage,
    REQUIRED_REPLAY_CLASS,
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
fn renders_committed_replay_readiness_status() {
    let rendered = render_replay_readiness_status("../..").expect("readiness renders");
    assert_eq!(
        rendered,
        include_str!("../../../docs/replay-readiness-status.md")
    );
    assert!(rendered.contains("Fresh workload authoring | `experimental`"));
    assert!(rendered.contains("Full Antithesis-style product replacement | `not-supported`"));
    check_replay_readiness_status("../..").expect("committed readiness report is fresh");
}

#[test]
fn rejects_stale_replay_readiness_status() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    std::fs::create_dir_all(root.join("docs")).expect("create docs");
    std::fs::write(root.join("docs/replay-readiness-status.md"), "stale\n")
        .expect("write stale doc");
    write_valid_minimal_coverage_fixture(root);

    let err = check_replay_readiness_status(root).expect_err("stale doc rejected");
    assert!(err.message().contains("readiness report stale"));
}

#[test]
fn rejects_empty_replay_readiness_manifest() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    std::fs::create_dir_all(root.join("dogfood-results")).expect("create dogfood-results");
    std::fs::write(
        root.join("dogfood-results/accepted-workload-proofs.json"),
        r#"{
          "schema_version": 1,
          "scope": "test",
          "anti_claims": [],
          "required_replay_class": "snapshot_backed_reproduced",
          "proofs": []
        }"#,
    )
    .expect("write manifest");

    let err = render_replay_readiness_status(root).expect_err("empty manifest rejected");
    assert!(err
        .message()
        .contains("manifest must contain at least two independent workload proofs"));
}

#[test]
fn validates_replay_readiness_surface_drift_in_rust() {
    let flake = include_str!("../../../flake.nix");
    let gates = validate_gate_metadata(flake).expect("flake static gate metadata matches");

    assert!(gates.contains(&"readiness-surface-drift".to_string()));
    run_readiness_surface_drift_selftest("../..").expect("surface drift selftest passes");
}

#[test]
fn renders_replay_readiness_operator_surfaces_in_rust() {
    let receipt = sample_replay_readiness_receipt(true, "passed");
    let summary = summarize_replay_readiness_receipt(&receipt).expect("receipt summarizes");
    let dashboard =
        render_replay_readiness_dashboard(&receipt, &summary).expect("dashboard renders");
    let readme_block = render_replay_readiness_readme_status_block(&summary);

    assert!(summary.contains("dogfood=rust-workload:pass"));
    assert!(dashboard.contains("snapshot_backed_reproduced"));
    assert!(dashboard.contains("not universal determinism"));
    assert!(readme_block.contains("bounded committed-evidence signal"));
}

#[test]
fn rejects_malformed_replay_readiness_operator_receipt() {
    let mut receipt = sample_replay_readiness_receipt(true, "passed");
    receipt["command"] = serde_json::json!("other");

    let err = summarize_replay_readiness_receipt(&receipt).expect_err("command mismatch rejected");
    assert!(err.message().contains("expected replay-readiness"));
}

#[test]
fn summarizes_sdk_local_adoption_tracks_in_rust() {
    let harness = "{\"antithesis_setup\":{\"status\":\"complete\",\"details\":{\"adoption_track\":\"external-harness\"}}}\n{\"antithesis_assert\":{\"assert_type\":\"always\",\"condition\":true,\"hit\":true,\"id\":\"1\",\"message\":\"driver invariant\",\"details\":{\"category\":\"driver\",\"adoption_track\":\"external-harness\"}}}\n";
    let in_process = "{\"antithesis_assert\":{\"assert_type\":\"always\",\"condition\":true,\"hit\":true,\"id\":\"2\",\"message\":\"internal invariant\",\"details\":{\"category\":\"service-invariant\",\"instrumentation_source\":\"in-process-service\"}}}\n";
    let report = summarize_sdk_local_jsonl(
        &format!("{harness}{in_process}"),
        "instrumentation-dry-run",
        None,
    )
    .expect("sdk local report summarizes");

    assert_eq!(report["adoption_tracks"]["external-harness"], 2);
    assert_eq!(report["adoption_tracks"]["in-process-service"], 1);
    assert_eq!(report["instrumentation_sources"], report["adoption_tracks"]);
    assert_eq!(report["replay_evidence"], false);
    assert_eq!(report["setup_complete"], true);
}

#[test]
fn rejects_invalid_sdk_local_jsonl() {
    let err = summarize_sdk_local_jsonl("not-json\n", "instrumentation-dry-run", None)
        .expect_err("invalid jsonl rejected");
    assert!(err.message().contains("invalid JSONL"));
}

#[test]
fn validates_dogfood_artifact_size_guard_in_rust() {
    let line = check_dogfood_artifact_sizes("../../dogfood-results", 50 * 1024 * 1024)
        .expect("committed dogfood artifacts are bounded");

    assert!(line.contains("dogfood artifact size guard ok"));
    run_dogfood_guards_selftest().expect("artifact size selftest passes");
}

#[test]
fn validates_accepted_dogfood_config_in_rust() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    std::fs::create_dir_all(root.join("dogfood-results/fake-proof")).expect("create proof dir");
    std::fs::write(
        root.join("dogfood-results/fake-proof/summary.json"),
        r#"{"accepted":true,"snapshot_probe_fail_after":25,"verdict":{"replay_class":"snapshot_backed_reproduced","replay_parent_depth":2}}"#,
    )
    .expect("write summary");
    std::fs::write(
        root.join("dogfood-results/accepted-workload-proofs.json"),
        r#"{
          "schema_version": 1,
          "scope": "test",
          "anti_claims": [],
          "required_replay_class": "snapshot_backed_reproduced",
          "proofs": [
            {"workload":"fake-workload","assertion_id":7,"evidence_dir":"dogfood-results/fake-proof","summary":"summary.json","bug":"bug.json","verdict":"verdict.json","snapshot":"snapshot.bin"}
          ]
        }"#,
    )
    .expect("write manifest");
    std::fs::write(
        root.join("dogfood-results/accepted-dogfood-expectations.json"),
        r#"{"workloads":{"fake-workload":{"assertion_id":7,"probe_key":"fake_probe","fail_after_key":"fake_fail_after","runner":{"fail_after_values":[25],"max_attempts":3},"expected":{"accepted":true,"replay_class":"snapshot_backed_reproduced","min_replay_parent_depth":2,"fail_after_values":[25]}}}}"#,
    )
    .expect("write expectations");
    std::fs::write(
        root.join("config.json"),
        r#"{"fake-workload":{"assertion_id":7,"fail_after_values":[25],"max_attempts":3,"cmdline_template":"fake_probe=snapshot_replay_probe fake_fail_after={fail_after}","expectation":{"assertion_id":7,"probe_key":"fake_probe","fail_after_key":"fake_fail_after","runner":{"fail_after_values":[25],"max_attempts":3},"expected":{"accepted":true,"replay_class":"snapshot_backed_reproduced","min_replay_parent_depth":2,"fail_after_values":[25]}}}}"#,
    )
    .expect("write config");

    let line = validate_accepted_dogfood_config(
        root.join("config.json"),
        root.join("dogfood-results/accepted-dogfood-expectations.json"),
        root.join("dogfood-results/accepted-workload-proofs.json"),
    )
    .expect("accepted dogfood config validates");
    assert!(line.contains("1 workloads match"));
}

#[test]
fn rejects_drifted_accepted_dogfood_config() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    std::fs::create_dir_all(root.join("dogfood-results/fake-proof")).expect("create proof dir");
    std::fs::write(
        root.join("dogfood-results/fake-proof/summary.json"),
        r#"{"accepted":true,"snapshot_probe_fail_after":25,"verdict":{"replay_class":"snapshot_backed_reproduced","replay_parent_depth":2}}"#,
    )
    .expect("write summary");
    std::fs::write(
        root.join("dogfood-results/accepted-workload-proofs.json"),
        r#"{"schema_version":1,"scope":"test","anti_claims":[],"required_replay_class":"snapshot_backed_reproduced","proofs":[{"workload":"fake-workload","assertion_id":7,"evidence_dir":"dogfood-results/fake-proof","summary":"summary.json","bug":"bug.json","verdict":"verdict.json","snapshot":"snapshot.bin"}]}"#,
    )
    .expect("write manifest");
    std::fs::write(
        root.join("dogfood-results/accepted-dogfood-expectations.json"),
        r#"{"workloads":{"fake-workload":{"assertion_id":7,"probe_key":"fake_probe","fail_after_key":"fake_fail_after","runner":{"fail_after_values":[25]},"expected":{"accepted":true,"replay_class":"snapshot_backed_reproduced","fail_after_values":[25]}}}}"#,
    )
    .expect("write expectations");
    std::fs::write(
        root.join("config.json"),
        r#"{"fake-workload":{"assertion_id":8,"fail_after_values":[30],"cmdline_template":"missing","expectation":{}}}"#,
    )
    .expect("write config");

    let err = validate_accepted_dogfood_config(
        root.join("config.json"),
        root.join("dogfood-results/accepted-dogfood-expectations.json"),
        root.join("dogfood-results/accepted-workload-proofs.json"),
    )
    .expect_err("drifted config rejected");
    assert!(err.message().contains("wrapper assertion_id"));
}

#[test]
fn renders_committed_assertion_readiness_status() {
    let rendered = render_assertion_readiness_status("../..").expect("assertion readiness renders");
    assert_eq!(
        rendered,
        include_str!("../../../docs/assertion-readiness-status.md")
    );
    assert!(rendered.contains("## Promotion guidance"));
    assert!(rendered.contains("rust-workload: 1 non-passing assertion(s)"));
    check_assertion_readiness_status("../..")
        .expect("committed assertion readiness report is fresh");
}

#[test]
fn rejects_stale_assertion_readiness_status() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    std::fs::create_dir_all(root.join("docs")).expect("create docs");
    std::fs::write(root.join("docs/assertion-readiness-status.md"), "stale\n")
        .expect("write stale doc");
    write_valid_minimal_coverage_fixture(root);
    write_assertions(&root.join("dogfood-results/fake-proof/assertions.json"));

    let err = check_assertion_readiness_status(root).expect_err("stale doc rejected");
    assert!(err.message().contains("assertion readiness report stale"));
}

#[test]
fn rejects_missing_assertions_for_assertion_readiness_status() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    write_valid_minimal_coverage_fixture(root);

    let err = render_assertion_readiness_status(root).expect_err("missing assertions rejected");
    assert!(err.message().contains("assertions.json"));
}

#[test]
fn validates_committed_assertion_readiness_promotion_gate() {
    let lines = check_assertion_readiness_promotion("../..").expect("promotion gate passes");

    assert!(lines
        .iter()
        .any(|line| line.contains("raft: cataloged=43 exercised=42")));
    assert!(lines
        .iter()
        .any(|line| line.contains("redb: cataloged=27 exercised=18")));
    run_assertion_readiness_promotion_selftest("../..").expect("selftest passes");
}

#[test]
fn rejects_assertion_readiness_overclaim() {
    let root = std::path::Path::new("../..");
    let manifest =
        AcceptedWorkloadProofs::from_path("../../dogfood-results/accepted-workload-proofs.json")
            .expect("manifest parses");
    let report = format!(
        "{}
assertion coverage proves replay.
",
        include_str!("../../../docs/assertion-readiness-status.md")
    );

    let err = validate_assertion_readiness_promotion(root, &manifest, &report)
        .expect_err("overclaim is rejected");
    assert!(err.message().contains("overclaim fragment"));
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
fn materializes_snapshot_chunks_and_rejects_existing_raw_without_force() {
    let temp = tempfile::tempdir().expect("tempdir");
    let manifest_path = write_snapshot_chunk_fixture(temp.path()).expect("fixture");

    let result = materialize_snapshot_chunks(&manifest_path, false).expect("materializes");
    assert_eq!(result.size, b"alpha-beta-gamma".len() as u64);
    assert!(result.path.exists());
    assert_eq!(
        std::fs::read(&result.path).expect("read snapshot"),
        b"alpha-beta-gamma"
    );
    assert!(result.render().contains("sha256:"));

    let err =
        materialize_snapshot_chunks(&manifest_path, false).expect_err("existing raw rejected");
    assert!(err.message().contains("raw snapshot already exists"));
}

#[test]
fn materialize_snapshot_chunks_selftest_covers_negative_fixtures() {
    run_materialize_snapshot_chunks_selftest().expect("selftest passes");
}

#[test]
fn rejects_unsafe_materialize_original_path() {
    let temp = tempfile::tempdir().expect("tempdir");
    let manifest_path = write_snapshot_chunk_fixture(temp.path()).expect("fixture");
    let mut manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).expect("read manifest"))
            .expect("manifest json");
    manifest["original_path"] = serde_json::Value::String("../escape.snapshot.bin".to_string());
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).expect("json"),
    )
    .expect("write manifest");

    let err = materialize_snapshot_chunks(&manifest_path, true).expect_err("unsafe path rejected");
    assert!(err
        .message()
        .contains("chunk manifest original_path must be a local snapshot filename"));
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

fn write_assertions(path: &std::path::Path) {
    std::fs::write(
        path,
        r#"[
          {"id":"a","message":"always hit","kind":"always","category":"uncategorized","hit_count":1,"verdict":"passed"},
          {"id":"b","message":"sometimes unhit","kind":"sometimes","category":"uncategorized","hit_count":0,"verdict":"failed"},
          {"id":"c","message":"reachable hit","kind":"reachable","category":"checked","hit_count":"2","verdict":"passed"}
        ]"#,
    )
    .expect("write assertions");
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
