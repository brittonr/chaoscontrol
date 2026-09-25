//! Release and dogfood gates refuse legacy-diagnostic frontier-policy outputs.

use chaoscontrol_evidence::frontier_policy_gate::{
    CHECKPOINT_FILE, FRONTIER_POLICY_KEY, FRONTIER_POLICY_RECEIPT_FILE,
    FRONTIER_POLICY_RECEIPT_SCHEMA, LEGACY_DIAGNOSTIC_FRONTIER_POLICY, LEGACY_DIAGNOSTIC_REFUSAL,
    RELEASE_ELIGIBLE_KEY, REPORT_FILE, REPORT_RELEASE_INELIGIBLE_LINE, RUN_CONFIG_FILE,
};
use chaoscontrol_evidence::{validate_accepted_dogfood_config, EvidenceError};
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;

const WORKLOAD: &str = "fake-workload";
const PROOF_DIR: &str = "dogfood-results/fake-proof";
const MATERIALIZE_FIXTURE_DIR: &str = "../../dogfood-results/raft-20260506-131815";
const MATERIALIZE_FIXTURE_FILES: [&str; 4] = [
    "assertions.json",
    "checkpoint.json",
    "receipt.md",
    "report.txt",
];
const MATERIALIZE_ARGS: [&str; 8] = [
    "--git-revision",
    "fixture",
    "--replay-status",
    "accepted",
    "--replay-message",
    "no-bug",
    "--replay-exit-status",
    "0",
];

fn diagnostic_receipt() -> Value {
    json!({
        "schema": FRONTIER_POLICY_RECEIPT_SCHEMA,
        FRONTIER_POLICY_KEY: LEGACY_DIAGNOSTIC_FRONTIER_POLICY,
        RELEASE_ELIGIBLE_KEY: false,
    })
}

fn write_json(path: &Path, value: &Value) {
    std::fs::write(path, serde_json::to_vec(value).expect("serialize fixture")).expect("write");
}

fn write_dogfood_config(root: &Path, wrapper_extra: &[(&str, Value)], bug: &str) {
    let dogfood = root.join(PROOF_DIR);
    std::fs::create_dir_all(&dogfood).expect("create proof dir");
    std::fs::write(
        dogfood.join("summary.json"),
        r#"{"accepted":true,"snapshot_probe_fail_after":25,"verdict":{"replay_class":"snapshot_backed_reproduced","replay_parent_depth":2}}"#,
    )
    .expect("write summary");
    std::fs::write(
        dogfood.join("assertions.json"),
        include_str!("fixtures/assertion-readiness/accepted-v2.json"),
    )
    .expect("write accepted v2 assertions");
    let expectation = json!({
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
    let mut wrapper = json!({
        "assertion_id": 7,
        "fail_after_values": [25],
        "max_attempts": 3,
        "cmdline_template": "fake_probe=snapshot_replay_probe fake_fail_after={fail_after}",
        "expectation": expectation.clone()
    });
    for (key, value) in wrapper_extra {
        wrapper[*key] = value.clone();
    }
    write_json(
        &root.join("dogfood-results/accepted-workload-proofs.json"),
        &json!({
            "schema_version": 1,
            "scope": "test",
            "anti_claims": [],
            "required_replay_class": "snapshot_backed_reproduced",
            "proofs": [{
                "workload": WORKLOAD,
                "assertion_id": 7,
                "evidence_dir": PROOF_DIR,
                "summary": "summary.json",
                "bug": bug,
                "verdict": "verdict.json",
                "snapshot": "snapshot.bin"
            }]
        }),
    );
    write_json(
        &root.join("dogfood-results/accepted-dogfood-expectations.json"),
        &json!({"workloads": {WORKLOAD: expectation}}),
    );
    write_json(&root.join("config.json"), &json!({ WORKLOAD: wrapper }));
}

fn check_dogfood_config(root: &Path) -> Result<String, EvidenceError> {
    validate_accepted_dogfood_config(
        root.join("config.json"),
        root.join("dogfood-results/accepted-dogfood-expectations.json"),
        root.join("dogfood-results/accepted-workload-proofs.json"),
    )
}

fn assert_legacy_refusal(message: &str, context: &str) {
    assert!(
        message.contains(LEGACY_DIAGNOSTIC_REFUSAL),
        "{context}: expected the legacy-diagnostic refusal, got: {message}"
    );
}

// r[verify chaoscontrol.campaign_policy.rollback.valid]
#[test]
fn accepted_dogfood_config_refuses_legacy_diagnostic_outputs() {
    let clean = tempfile::tempdir().expect("tempdir");
    write_dogfood_config(clean.path(), &[], "bug.json");
    check_dogfood_config(clean.path()).expect("Campaign-era proof is admitted");

    let explicit_campaign = tempfile::tempdir().expect("tempdir");
    write_dogfood_config(
        explicit_campaign.path(),
        &[(FRONTIER_POLICY_KEY, json!("campaign"))],
        "bug.json",
    );
    check_dogfood_config(explicit_campaign.path()).expect("explicit Campaign is admitted");

    let wrapper_cases: [(&str, &[(&str, Value)]); 2] = [
        (
            "wrapper frontier_policy",
            &[(
                FRONTIER_POLICY_KEY,
                json!(LEGACY_DIAGNOSTIC_FRONTIER_POLICY),
            )],
        ),
        (
            "wrapper release_eligible",
            &[(RELEASE_ELIGIBLE_KEY, json!(false))],
        ),
    ];
    for (context, extra) in wrapper_cases {
        let temp = tempfile::tempdir().expect("tempdir");
        write_dogfood_config(temp.path(), extra, "bug.json");
        let error = check_dogfood_config(temp.path()).expect_err(context);
        assert_legacy_refusal(error.message(), context);
    }

    let temp = tempfile::tempdir().expect("tempdir");
    write_dogfood_config(temp.path(), &[], FRONTIER_POLICY_RECEIPT_FILE);
    let error = check_dogfood_config(temp.path()).expect_err("proof bug names the receipt");
    assert_legacy_refusal(error.message(), "proof.bug");

    let evidence_cases = [
        (FRONTIER_POLICY_RECEIPT_FILE, diagnostic_receipt()),
        (
            CHECKPOINT_FILE,
            json!({"config": {FRONTIER_POLICY_KEY: LEGACY_DIAGNOSTIC_FRONTIER_POLICY}, RELEASE_ELIGIBLE_KEY: false}),
        ),
        (
            RUN_CONFIG_FILE,
            json!({FRONTIER_POLICY_KEY: LEGACY_DIAGNOSTIC_FRONTIER_POLICY}),
        ),
    ];
    for (file, contents) in evidence_cases {
        let temp = tempfile::tempdir().expect("tempdir");
        write_dogfood_config(temp.path(), &[], "bug.json");
        write_json(&temp.path().join(PROOF_DIR).join(file), &contents);
        let error = check_dogfood_config(temp.path()).expect_err(file);
        assert_legacy_refusal(error.message(), file);
    }
}

fn materialize_output(directory: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_materialize-dogfood-receipt"))
        .arg(directory)
        .args(MATERIALIZE_ARGS)
        .output()
        .expect("run materialize-dogfood-receipt")
}

fn copy_materialize_fixture(directory: &Path) {
    for file in MATERIALIZE_FIXTURE_FILES {
        std::fs::copy(
            Path::new(MATERIALIZE_FIXTURE_DIR).join(file),
            directory.join(file),
        )
        .expect("copy dogfood fixture");
    }
}

// r[verify chaoscontrol.campaign_policy.rollback.valid]
#[test]
fn materialize_dogfood_receipt_refuses_legacy_diagnostic_outputs() {
    let clean = tempfile::tempdir().expect("tempdir");
    copy_materialize_fixture(clean.path());
    let output = materialize_output(clean.path());
    assert!(
        output.status.success(),
        "Campaign-era output materializes: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(clean.path().join("receipt.json").is_file());

    let checkpoint_text =
        std::fs::read_to_string(Path::new(MATERIALIZE_FIXTURE_DIR).join(CHECKPOINT_FILE))
            .expect("read fixture checkpoint");
    let mut diagnostic_checkpoint: Value =
        serde_json::from_str(&checkpoint_text).expect("fixture checkpoint parses");
    diagnostic_checkpoint["config"][FRONTIER_POLICY_KEY] = json!(LEGACY_DIAGNOSTIC_FRONTIER_POLICY);
    diagnostic_checkpoint[RELEASE_ELIGIBLE_KEY] = json!(false);
    let report_text = std::fs::read_to_string(Path::new(MATERIALIZE_FIXTURE_DIR).join(REPORT_FILE))
        .expect("read fixture report");
    let marks: [(&str, Vec<u8>); 3] = [
        (
            FRONTIER_POLICY_RECEIPT_FILE,
            serde_json::to_vec(&diagnostic_receipt()).expect("serialize receipt"),
        ),
        (
            CHECKPOINT_FILE,
            serde_json::to_vec(&diagnostic_checkpoint).expect("serialize checkpoint"),
        ),
        (
            REPORT_FILE,
            format!("{report_text}{REPORT_RELEASE_INELIGIBLE_LINE}\n").into_bytes(),
        ),
    ];
    for (file, contents) in marks {
        let directory = tempfile::tempdir().expect("tempdir");
        copy_materialize_fixture(directory.path());
        std::fs::write(directory.path().join(file), contents).expect("write diagnostic mark");
        let output = materialize_output(directory.path());
        assert!(
            !output.status.success(),
            "{file}: diagnostic output refused"
        );
        assert_legacy_refusal(&String::from_utf8_lossy(&output.stderr), file);
        for written in [RUN_CONFIG_FILE, "receipt.json"] {
            assert!(
                !directory.path().join(written).exists(),
                "{file}: refusal happens before {written} is written"
            );
        }
    }
}
