//! Product evidence admission refuses Campaign/Choregraph structural artifacts.

use chaoscontrol_evidence::campaign_evidence_boundary::STRUCTURAL_EVIDENCE_REFUSAL;
use chaoscontrol_evidence::{
    default_readiness_promotion_paths, validate_accepted_dogfood_config,
    validate_readiness_promotion, validate_receipt, validate_replay_verdict_with_options,
    validate_snapshot_ref, AcceptedWorkloadProofs, EvidenceError,
};
use serde_json::Value;
use std::path::Path;

const VALID_RECEIPT: &str =
    include_str!("../../../contracts/evidence/fixtures/valid/receipt.known-gap.valid.json");
const VALID_VERDICT: &str = include_str!(
    "../../../contracts/evidence/fixtures/valid/replay-verdict.snapshot-backed.valid.json"
);
const VALID_SNAPSHOT_REF: &str =
    include_str!("../../../contracts/evidence/fixtures/valid/snapshot-ref.valid.json");
const REPO_ROOT: &str = "../..";
const RUN_DIR: &str = "dogfood-results/raft-20260506-095025";
const JOURNAL_SEGMENT: &str = "campaign-journal/epoch-0000/seg-00000000.cbor";
const JOURNAL_LOCK: &str = "campaign-journal/LOCK";
const JOURNAL_DIRECTORY: &str = "campaign-journal";
const FRONTIER_TRACE: &str = "frontier-trace.jsonl";
const STRUCTURAL_ARTIFACTS: [&str; 4] = [
    JOURNAL_SEGMENT,
    JOURNAL_LOCK,
    JOURNAL_DIRECTORY,
    FRONTIER_TRACE,
];
const STRUCTURAL_SCHEMAS: [&str; 4] = [
    "chaoscontrol.campaign-journal-segment.v1",
    "chaoscontrol.frontier-trace.v1",
    "campaign.provenance.v1",
    "choregraph.nickel-source-receipt.v1",
];
const RECEIPT_REFERENCE_POINTERS: [&str; 5] = [
    "/config/path",
    "/artifact_hashes/0/path",
    "/assertion_coverage/summary_path",
    "/bug_reports/0/path",
    "/checkpoint_reference/path",
];
const PROOF_REFERENCE_FIELDS: [&str; 6] = [
    "evidence_dir",
    "summary",
    "bug",
    "verdict",
    "snapshot",
    "receipt",
];

fn parse(text: &str) -> Value {
    serde_json::from_str(text).expect("fixture parses")
}

fn assert_structural_refusal(error: &EvidenceError, context: &str) {
    assert!(
        error.message().contains(STRUCTURAL_EVIDENCE_REFUSAL),
        "{context}: expected Campaign evidence boundary refusal, got: {}",
        error.message()
    );
}

fn set(value: &mut Value, pointer: &str, replacement: &str) {
    *value
        .pointer_mut(pointer)
        .unwrap_or_else(|| panic!("fixture has {pointer}")) = Value::from(replacement);
}

// r[verify chaoscontrol.campaign_policy.evidence.valid]
#[test]
fn product_receipt_verdict_and_snapshot_ref_are_admitted() {
    validate_receipt(&parse(VALID_RECEIPT)).expect("product receipt is admitted");
    validate_replay_verdict_with_options(&parse(VALID_VERDICT), true, false, Path::new("."))
        .expect("product replay verdict is admitted");
    validate_snapshot_ref(&parse(VALID_SNAPSHOT_REF)).expect("product snapshot ref is admitted");
}

// r[verify chaoscontrol.campaign_policy.evidence.invalid]
#[test]
fn receipt_refuses_structural_artifacts_in_every_reference_field() {
    for pointer in RECEIPT_REFERENCE_POINTERS {
        for artifact in STRUCTURAL_ARTIFACTS {
            let mut receipt = parse(VALID_RECEIPT);
            set(&mut receipt, pointer, &format!("{RUN_DIR}/{artifact}"));
            let error = validate_receipt(&receipt).expect_err("structural receipt reference");
            assert_structural_refusal(&error, &format!("{pointer} = {artifact}"));
        }
    }
}

// r[verify chaoscontrol.campaign_policy.evidence.invalid]
#[test]
fn receipt_refuses_campaign_and_choregraph_schemas() {
    for schema in STRUCTURAL_SCHEMAS {
        let mut receipt = parse(VALID_RECEIPT);
        receipt["schema_version"] = Value::from(schema);
        let error = validate_receipt(&receipt).expect_err("structural schema_version");
        assert_structural_refusal(&error, schema);

        let mut receipt = parse(VALID_RECEIPT);
        receipt["schema"] = Value::from(schema);
        let error = validate_receipt(&receipt).expect_err("structural schema");
        assert_structural_refusal(&error, schema);
    }
}

// r[verify chaoscontrol.campaign_policy.evidence.invalid]
#[test]
fn replay_verdict_refuses_structural_bug_and_artifact_paths() {
    for artifact in STRUCTURAL_ARTIFACTS {
        let path = format!("dogfood-results/raft-snapshot-probe/{artifact}");
        for pointer in ["/bug_path", "/artifact_hashes/0/path"] {
            let mut verdict = parse(VALID_VERDICT);
            set(&mut verdict, pointer, &path);
            for accepted in [false, true] {
                let error =
                    validate_replay_verdict_with_options(&verdict, accepted, false, Path::new("."))
                        .expect_err("structural replay verdict reference");
                assert_structural_refusal(&error, &format!("{pointer} = {artifact}"));
            }
        }
    }
}

// r[verify chaoscontrol.campaign_policy.evidence.invalid]
#[test]
fn snapshot_ref_refuses_structural_artifacts_under_snapshots() {
    for artifact in STRUCTURAL_ARTIFACTS {
        let mut reference = parse(VALID_SNAPSHOT_REF);
        reference["path"] = Value::from(format!("snapshots/{artifact}"));
        let error = validate_snapshot_ref(&reference).expect_err("structural snapshot ref");
        assert_structural_refusal(&error, artifact);
    }
}

// r[verify chaoscontrol.campaign_policy.evidence.valid]
// r[verify chaoscontrol.campaign_policy.evidence.invalid]
#[test]
fn accepted_proof_manifest_refuses_structural_references() {
    let (manifest_path, report_path) = default_readiness_promotion_paths(REPO_ROOT);
    let manifest_text = std::fs::read_to_string(&manifest_path).expect("read manifest");
    let report = std::fs::read_to_string(&report_path).expect("read readiness report");
    let manifest: Value = parse(&manifest_text);
    AcceptedWorkloadProofs::from_json_str(&manifest_text)
        .expect("committed manifest parses")
        .validate_shape()
        .expect("committed product manifest is admitted");
    validate_readiness_promotion(&manifest, &report).expect("committed readiness gate passes");

    for field in PROOF_REFERENCE_FIELDS {
        for artifact in STRUCTURAL_ARTIFACTS {
            let mut structural = manifest.clone();
            structural["proofs"][0][field] = Value::from(artifact);
            let context = format!("proof.{field} = {artifact}");

            let typed = AcceptedWorkloadProofs::from_json_str(&structural.to_string())
                .expect("mutated manifest parses");
            let error = typed.validate_shape().expect_err("typed manifest refusal");
            assert_structural_refusal(&error, &context);

            let error = validate_readiness_promotion(&structural, &report)
                .expect_err("readiness promotion refusal");
            assert_structural_refusal(&error, &context);
        }
    }
}

fn write_dogfood_config(root: &Path, bug: &str) {
    let dogfood = root.join("dogfood-results");
    std::fs::create_dir_all(dogfood.join("fake-proof")).expect("create proof dir");
    std::fs::write(
        dogfood.join("fake-proof/summary.json"),
        r#"{"accepted":true,"snapshot_probe_fail_after":25,"verdict":{"replay_class":"snapshot_backed_reproduced","replay_parent_depth":2}}"#,
    )
    .expect("write summary");
    std::fs::write(
        dogfood.join("fake-proof/assertions.json"),
        include_str!("fixtures/assertion-readiness/accepted-v2.json"),
    )
    .expect("write accepted v2 assertions");
    let manifest = serde_json::json!({
        "schema_version": 1,
        "scope": "test",
        "anti_claims": [],
        "required_replay_class": "snapshot_backed_reproduced",
        "proofs": [{
            "workload": "fake-workload",
            "assertion_id": 7,
            "evidence_dir": "dogfood-results/fake-proof",
            "summary": "summary.json",
            "bug": bug,
            "verdict": "verdict.json",
            "snapshot": "snapshot.bin"
        }]
    });
    let expectation = serde_json::json!({
        "assertion_id": 7,
        "probe_key": "fake_probe",
        "fail_after_key": "fake_fail_after",
        "runner": {"fail_after_values": [25], "max_attempts": 3},
        "expected": {
            "accepted": true,
            "replay_class": "snapshot_backed_reproduced",
            "min_replay_parent_depth": 2,
            "fail_after_values": [25]
        }
    });
    let config = serde_json::json!({
        "fake-workload": {
            "assertion_id": 7,
            "fail_after_values": [25],
            "max_attempts": 3,
            "cmdline_template": "fake_probe=snapshot_replay_probe fake_fail_after={fail_after}",
            "expectation": expectation.clone()
        }
    });
    for (path, value) in [
        (dogfood.join("accepted-workload-proofs.json"), manifest),
        (
            dogfood.join("accepted-dogfood-expectations.json"),
            serde_json::json!({"workloads": {"fake-workload": expectation}}),
        ),
        (root.join("config.json"), config),
    ] {
        std::fs::write(path, serde_json::to_vec(&value).expect("serialize fixture"))
            .expect("write fixture");
    }
}

fn check_dogfood_config(root: &Path) -> Result<String, EvidenceError> {
    validate_accepted_dogfood_config(
        root.join("config.json"),
        root.join("dogfood-results/accepted-dogfood-expectations.json"),
        root.join("dogfood-results/accepted-workload-proofs.json"),
    )
}

// r[verify chaoscontrol.campaign_policy.evidence.valid]
// r[verify chaoscontrol.campaign_policy.evidence.invalid]
#[test]
fn accepted_dogfood_config_refuses_structural_proof_references() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_dogfood_config(temp.path(), "bug.json");
    check_dogfood_config(temp.path()).expect("product proof references are admitted");

    for artifact in STRUCTURAL_ARTIFACTS {
        let temp = tempfile::tempdir().expect("tempdir");
        write_dogfood_config(temp.path(), artifact);
        let error = check_dogfood_config(temp.path()).expect_err("structural proof bug refused");
        assert_structural_refusal(&error, artifact);
    }
}
