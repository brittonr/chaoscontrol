//! Pure replay-readiness receipt projection.

// r[impl chaoscontrol.rust_automation.evidence]
// r[impl chaoscontrol.rust_automation.nix]

use serde_json::{json, Value};

pub const GATE_SPECS: [(&str, &str, &str); 13] = [
    (
        "contract-registry",
        "check-contract-registry .",
        "CONTRACT_REGISTRY_STATUS",
    ),
    (
        "evidence-contracts",
        "check-evidence-contracts --root .",
        "EVIDENCE_CONTRACTS_STATUS",
    ),
    (
        "replay-proof-coverage",
        "check-replay-proof-coverage .",
        "REPLAY_PROOF_COVERAGE_STATUS",
    ),
    (
        "readiness-promotion",
        "check-readiness-promotion-gate --root .",
        "READINESS_PROMOTION_STATUS",
    ),
    (
        "readiness-surface-drift",
        "check-readiness-surface-drift .",
        "READINESS_SURFACE_DRIFT_STATUS",
    ),
    (
        "readiness-report",
        "generate-replay-readiness-report --check .",
        "READINESS_REPORT_STATUS",
    ),
    (
        "assertion-readiness-report",
        "generate-assertion-readiness-report --check .",
        "ASSERTION_REPORT_STATUS",
    ),
    (
        "assertion-readiness-boundary",
        "check-assertion-readiness-boundary .",
        "ASSERTION_PROMOTION_STATUS",
    ),
    (
        "sdk-local-report-tracks",
        "check-sdk-local-report-tracks",
        "SDK_LOCAL_REPORT_TRACKS_STATUS",
    ),
    (
        "sdk-assertion-quality",
        "check-sdk-assertion-quality",
        "SDK_ASSERTION_QUALITY_STATUS",
    ),
    (
        "consistency-checker-fixtures",
        "check-consistency-fixtures .",
        "CONSISTENCY_FIXTURES_STATUS",
    ),
    (
        "dogfood-artifact-sizes",
        "check-dogfood-artifact-sizes",
        "ARTIFACT_SIZES_STATUS",
    ),
    (
        "accepted-dogfood-config",
        "check-accepted-dogfood-config --config <nix-generated>",
        "ACCEPTED_DOGFOOD_CONFIG_STATUS",
    ),
];

pub fn gate_names() -> Vec<String> {
    GATE_SPECS
        .iter()
        .map(|(name, _, _)| (*name).to_string())
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateInput {
    pub name: String,
    pub command: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptInput {
    pub status: String,
    pub failed_phase: Option<String>,
    pub exit_code: i32,
    pub started_at: String,
    pub finished_at: String,
    pub dogfood: Option<String>,
    pub dogfood_status: String,
    pub dogfood_output: Option<String>,
    pub dogfood_summary: Option<Value>,
    pub gates: Vec<GateInput>,
}

pub fn build_receipt(input: &ReceiptInput, expectations: &Value) -> Result<Value, String> {
    let expectation = load_expectation(input.dogfood.as_deref(), expectations)?;
    let expectation_status =
        expectation_status(expectation.as_ref(), input.dogfood_summary.as_ref())?;
    let gates = input
        .gates
        .iter()
        .map(|gate| json!({"name": gate.name, "command": gate.command, "status": gate.status}))
        .collect::<Vec<_>>();
    Ok(json!({
        "schema_version": 1,
        "command": "replay-readiness",
        "status": input.status,
        "exit_code": input.exit_code,
        "failed_phase": input.failed_phase,
        "started_at": input.started_at,
        "finished_at": input.finished_at,
        "static_gates": gates,
        "dogfood": {
            "selected_workload": input.dogfood,
            "status": input.dogfood_status,
            "output": input.dogfood_output,
            "summary": input.dogfood_summary,
            "expectation": expectation,
            "expectation_status": expectation_status,
            "evidence_curation": "explicit-follow-up",
        },
        "scope": "bounded committed replay/evidence readiness; not universal determinism or hosted-product parity",
    }))
}

fn load_expectation(workload: Option<&str>, root: &Value) -> Result<Option<Value>, String> {
    let Some(workload) = workload else {
        return Ok(None);
    };
    let expectation = root
        .get("workloads")
        .and_then(Value::as_object)
        .and_then(|workloads| workloads.get(workload))
        .cloned()
        .ok_or_else(|| format!("missing dogfood expectation for {workload}"))?;
    Ok(Some(expectation))
}

fn expectation_status(
    expectation: Option<&Value>,
    summary: Option<&Value>,
) -> Result<String, String> {
    let Some(expectation) = expectation else {
        return Ok(String::from("not-applicable"));
    };
    let Some(summary) = summary else {
        return Ok(String::from("not-observed"));
    };
    let expected = expectation
        .get("expected")
        .and_then(Value::as_object)
        .ok_or_else(|| String::from("dogfood expectation lacks expected object"))?;
    let mut mismatches = Vec::new();
    if summary.get("accepted") != expected.get("accepted") {
        mismatches.push("accepted");
    }
    let verdict = summary.get("verdict").and_then(Value::as_object);
    if verdict.and_then(|value| value.get("replay_class")) != expected.get("replay_class") {
        mismatches.push("replay_class");
    }
    if let Some(minimum) = expected
        .get("min_replay_parent_depth")
        .and_then(Value::as_i64)
    {
        let depth = verdict
            .and_then(|value| value.get("replay_parent_depth"))
            .and_then(Value::as_i64);
        if depth.is_none_or(|depth| depth < minimum) {
            mismatches.push("replay_parent_depth");
        }
    }
    if let Some(seeds) = expected.get("allowed_seeds").and_then(Value::as_array) {
        if !seeds.contains(summary.get("seed").unwrap_or(&Value::Null)) {
            mismatches.push("seed");
        }
    }
    if let Some(values) = expected.get("fail_after_values").and_then(Value::as_array) {
        if !values.contains(
            summary
                .get("snapshot_probe_fail_after")
                .unwrap_or(&Value::Null),
        ) {
            mismatches.push("fail_after");
        }
    }
    if mismatches.is_empty() {
        Ok(String::from("matched"))
    } else {
        Ok(format!("mismatched:{}", mismatches.join(",")))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{build_receipt, GateInput, ReceiptInput};

    fn input() -> ReceiptInput {
        ReceiptInput {
            status: String::from("passed"),
            failed_phase: None,
            exit_code: 0,
            started_at: String::from("start"),
            finished_at: String::from("finish"),
            dogfood: Some(String::from("raft")),
            dogfood_status: String::from("pass"),
            dogfood_output: Some(String::from("/tmp/out")),
            dogfood_summary: Some(json!({
                "accepted": true,
                "seed": 42,
                "snapshot_probe_fail_after": 1,
                "verdict": {"replay_class": "snapshot_backed_reproduced", "replay_parent_depth": 1}
            })),
            gates: vec![GateInput {
                name: String::from("gate"),
                command: String::from("check"),
                status: String::from("pass"),
            }],
        }
    }

    #[test]
    fn matched_receipt_preserves_schema() {
        let expectations = json!({"workloads": {"raft": {"expected": {
            "accepted": true, "replay_class": "snapshot_backed_reproduced",
            "min_replay_parent_depth": 1, "allowed_seeds": [42], "fail_after_values": [1]
        }}}});
        let receipt = build_receipt(&input(), &expectations).expect("receipt");
        assert_eq!(receipt["dogfood"]["expectation_status"], "matched");
        assert_eq!(receipt["static_gates"][0]["name"], "gate");
    }

    #[test]
    fn missing_and_mismatched_expectations_fail_closed() {
        assert!(build_receipt(&input(), &json!({"workloads": {}}))
            .expect_err("missing")
            .contains("missing dogfood expectation"));
        let expectations = json!({"workloads": {"raft": {"expected": {
            "accepted": false, "replay_class": "other", "min_replay_parent_depth": 2,
            "allowed_seeds": [7], "fail_after_values": [9]
        }}}});
        let receipt = build_receipt(&input(), &expectations).expect("receipt");
        assert!(receipt["dogfood"]["expectation_status"]
            .as_str()
            .expect("status")
            .starts_with("mismatched:"));
    }
}
