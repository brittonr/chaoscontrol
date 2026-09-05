//! Pure VM determinism receipt summaries and checks.

// r[impl chaoscontrol.rust_automation.evidence]
// r[impl chaoscontrol.rust_automation.nix]

use serde_json::Value;

const EXPECTED_DRIFT_CASES: [&str; 4] = [
    "controller-3vm-1vcpu",
    "controller-3vm-2vcpu",
    "single-vm-1vcpu",
    "single-vm-2vcpu",
];
const EXPECTED_DRIFT_RUNS: u64 = 5;

pub fn matrix_summary(receipt: &Value) -> Result<String, String> {
    let rows = receipt
        .get("rows")
        .and_then(Value::as_array)
        .ok_or_else(|| String::from("rows must be a list"))?;
    let passed = receipt.get("passed").and_then(Value::as_bool) == Some(true);
    let mut lines = vec![
        format!(
            "vm determinism matrix: {}",
            if passed { "pass" } else { "fail" }
        ),
        format!("matrix_id: {}", display(receipt.get("matrix_id"))),
        format!("gate: {}", display(receipt.get("gate"))),
        format!("rows: {}", rows.len()),
        format!("scope: {}", display(receipt.get("scope"))),
    ];
    for row in rows {
        let profile = row.get("profile").and_then(Value::as_object);
        let report = row.get("report").and_then(Value::as_object);
        let mismatches = report
            .and_then(|value| value.get("mismatches"))
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        lines.push(format!(
            "- {}: status={} passed={} runs={} product={} workers={} workload={} kernel={} initrd={} device={} clock={} controller={} hypervisor={} mismatches={}",
            field(profile, "row_id"),
            display(row.get("status")),
            field(report, "passed"),
            field(report, "runs"),
            field(profile, "local_product_profile"),
            field(profile, "worker_count"),
            field(profile, "workload"),
            field(profile, "kernel_fingerprint"),
            field(profile, "initrd_fingerprint"),
            field(profile, "device_profile"),
            field(profile, "clock_profile"),
            field(profile, "controller_profile"),
            field(profile, "hypervisor_profile"),
            mismatches,
        ));
    }
    Ok(lines.join("\n"))
}

pub fn validate_drift_receipt(receipt: &Value) -> Result<String, String> {
    require(
        receipt.get("schema_version").and_then(Value::as_u64) == Some(1),
        "schema_version must be 1",
    )?;
    require(
        receipt.get("gate").and_then(Value::as_str) == Some("vm-determinism-drift"),
        "unexpected gate",
    )?;
    require(
        prefix(receipt.get("kernel_crc32"), "crc32:"),
        "missing kernel_crc32",
    )?;
    require(
        prefix(receipt.get("initrd_crc32"), "crc32:"),
        "missing initrd_crc32",
    )?;
    let cases = receipt
        .get("cases")
        .and_then(Value::as_array)
        .filter(|values| !values.is_empty())
        .ok_or_else(|| String::from("cases must be a non-empty list"))?;
    let seen = cases
        .iter()
        .filter_map(|case| case.get("name").and_then(Value::as_str))
        .collect::<std::collections::BTreeSet<_>>();
    let expected = EXPECTED_DRIFT_CASES
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    require(seen == expected, &format!("unexpected cases: {seen:?}"))?;
    let mut lines = vec![String::from("vm-determinism-drift receipt: pass")];
    for case in cases {
        let name = case
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        require(
            case.get("runs").and_then(Value::as_u64) == Some(EXPECTED_DRIFT_RUNS),
            &format!("{name}: expected 5 runs"),
        )?;
        require(
            case.get("passed").and_then(Value::as_bool) == Some(true),
            &format!("{name}: not passed"),
        )?;
        require(
            empty_array(case.get("mismatches")),
            &format!("{name}: mismatches present"),
        )?;
        require(
            case.get("dlog_structural_match").and_then(Value::as_bool) == Some(true),
            &format!("{name}: dlog structural mismatch"),
        )?;
        require(
            empty_array(case.get("dlog_mismatches")),
            &format!("{name}: dlog mismatches present"),
        )?;
        require(
            empty_array(case.get("dlog_divergences")),
            &format!("{name}: dlog divergences present"),
        )?;
        let observations = case
            .get("observations")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("{name}: observations must be a list"))?;
        require(
            observations.len() as u64 == EXPECTED_DRIFT_RUNS,
            &format!("{name}: observation count != runs"),
        )?;
        lines.push(format!(
            "{name}: {EXPECTED_DRIFT_RUNS} runs, mismatches=0, dlog_structural_match=true"
        ));
    }
    Ok(lines.join("\n"))
}

fn display(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(value)) => value.clone(),
        Some(value) => value.to_string(),
        None => String::from("null"),
    }
}

fn field(object: Option<&serde_json::Map<String, Value>>, name: &str) -> String {
    display(object.and_then(|value| value.get(name)))
}

fn prefix(value: Option<&Value>, expected: &str) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|value| value.starts_with(expected))
}

fn empty_array(value: Option<&Value>) -> bool {
    value.and_then(Value::as_array).is_some_and(Vec::is_empty)
}

fn require(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.to_string())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{matrix_summary, validate_drift_receipt, EXPECTED_DRIFT_CASES};

    #[test]
    fn matrix_summary_preserves_public_lines() {
        let value =
            json!({"passed": true, "matrix_id": "m", "gate": "g", "scope": "bounded", "rows": []});
        assert_eq!(
            matrix_summary(&value).expect("summary"),
            "vm determinism matrix: pass\nmatrix_id: m\ngate: g\nrows: 0\nscope: bounded"
        );
    }

    #[test]
    fn drift_positive_and_tampered_negative_are_distinct() {
        let cases = EXPECTED_DRIFT_CASES.map(|name| {
            json!({
                "name": name, "runs": 5, "passed": true, "mismatches": [],
                "dlog_structural_match": true, "dlog_mismatches": [], "dlog_divergences": [],
                "observations": [{}, {}, {}, {}, {}]
            })
        });
        let mut value = json!({"schema_version": 1, "gate": "vm-determinism-drift", "kernel_crc32": "crc32:a", "initrd_crc32": "crc32:b", "cases": cases});
        assert!(validate_drift_receipt(&value)
            .expect("valid")
            .contains("receipt: pass"));
        value["cases"][0]["passed"] = json!(false);
        assert!(validate_drift_receipt(&value)
            .expect_err("tampered")
            .contains("not passed"));
    }
}
