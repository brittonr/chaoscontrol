use chaoscontrol_evidence::validate_replay_verdict;
use serde_json::Value;

const VALID_VERDICT: &str = include_str!(
    "../../../contracts/evidence/fixtures/valid/replay-verdict.snapshot-backed.valid.json"
);

fn verdict() -> Value {
    serde_json::from_str(VALID_VERDICT).expect("valid replay verdict fixture parses")
}

#[test]
fn accepts_coherent_v2_replay_verdict() {
    validate_replay_verdict(&verdict()).expect("coherent v2 replay verdict is accepted");
}

#[test]
fn rejects_exit_status_that_conflicts_with_reproduction() {
    let mut value = verdict();
    value["command"]["exit_status"] = Value::from(1);

    let error = validate_replay_verdict(&value).expect_err("conflicting exit status is rejected");
    assert!(error.message().contains("exit status conflicts"));
}

#[test]
fn rejects_partial_bug_binding() {
    let mut value = verdict();
    value
        .as_object_mut()
        .expect("verdict object")
        .remove("bug_id");

    let error = validate_replay_verdict(&value).expect_err("partial binding is rejected");
    assert!(error.message().contains("binding must be complete"));
}

#[test]
fn rejects_snapshot_status_field_conflict() {
    let mut value = verdict();
    value["snapshot"]["present"] = Value::Bool(false);

    let error = validate_replay_verdict(&value).expect_err("snapshot conflict is rejected");
    assert!(error.message().contains("snapshot status fields conflict"));
}

#[test]
fn rejects_legacy_snapshot_codec_in_v2_verdict() {
    let mut value = verdict();
    value["snapshot"]["reference"]["codec"] =
        Value::String("simulation-snapshot-bincode-zstd-v1".to_string());
    value["snapshot"]["reference"]["schema_version"] = Value::from(1);

    let error = validate_replay_verdict(&value).expect_err("legacy codec is rejected for v2");
    assert!(error.message().contains("current CBOR v2 snapshot codec"));
}

#[test]
fn rejects_duplicate_artifact_paths() {
    let mut value = verdict();
    let bug_path = value["artifact_hashes"][0]["path"].clone();
    value["artifact_hashes"][1]["path"] = bug_path;

    let error = validate_replay_verdict(&value).expect_err("duplicate path is rejected");
    assert!(error.message().contains("duplicate artifact path"));
}

#[test]
fn rejects_replay_class_that_conflicts_with_snapshot() {
    let mut value = verdict();
    value["replay_class"] = Value::String("schedule_only_replay_gap".to_string());
    value["reproduced"] = Value::Bool(false);
    value["command"]["exit_status"] = Value::from(1);

    let error = validate_replay_verdict(&value).expect_err("class conflict is rejected");
    assert!(error.message().contains("class fields conflict"));
}
