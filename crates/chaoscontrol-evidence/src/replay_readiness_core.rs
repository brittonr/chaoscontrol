//! Pure replay-readiness classification over in-memory evidence facts.

// r[impl chaoscontrol.architecture_modules.evidence]
// r[impl chaoscontrol.architecture_modules.boundary]

pub fn summarize_receipt(receipt: &::serde_json::Value) -> crate::EvidenceResult<String> {
    let command = str_field(receipt.get("command"), "receipt.command")?;
    crate::ensure(
        command == "replay-readiness",
        format!("receipt.command: expected replay-readiness, got {command:?}"),
    )?;
    let status = str_field(receipt.get("status"), "receipt.status")?;
    crate::ensure(
        matches!(status, "passed" | "failed"),
        format!("receipt.status: unsupported value {status:?}"),
    )?;
    let gates = array_field(receipt.get("static_gates"), "receipt.static_gates")?;
    crate::ensure(
        !gates.is_empty(),
        "receipt.static_gates: expected non-empty list",
    )?;

    let mut passed_gates = 0usize;
    let mut failed_gates = Vec::new();
    for (index, gate) in gates.iter().enumerate() {
        let name = token_field(
            gate.get("name"),
            &format!("receipt.static_gates[{index}].name"),
        )?;
        let gate_status = str_field(
            gate.get("status"),
            &format!("receipt.static_gates[{index}].status"),
        )?;
        match gate_status {
            "pass" => passed_gates += 1,
            "fail" => failed_gates.push(name.to_string()),
            "pending" | "running" => {}
            other => {
                return Err(crate::EvidenceError::new(format!(
                    "receipt.static_gates[{index}].status: unsupported value {other:?}"
                )));
            }
        }
    }

    let dogfood = object_field(receipt.get("dogfood"), "receipt.dogfood")?;
    let selected = optional_token(
        dogfood.get("selected_workload"),
        "receipt.dogfood.selected_workload",
    )?;
    let dogfood_status = str_field(dogfood.get("status"), "receipt.dogfood.status")?;
    crate::ensure(
        matches!(dogfood_status, "skipped" | "pass" | "fail" | "running"),
        format!("receipt.dogfood.status: unsupported value {dogfood_status:?}"),
    )?;
    let failed_phase = optional_token(receipt.get("failed_phase"), "receipt.failed_phase")?;
    let exit_code = int_field(receipt.get("exit_code"), "receipt.exit_code")?;
    let scope = str_field(receipt.get("scope"), "receipt.scope")?;
    let scope_token = if scope.contains("bounded") && scope.contains("not universal") {
        "bounded"
    } else {
        "check-scope"
    };

    let mut dogfood_label = selected
        .map(|selected| format!("{selected}:{dogfood_status}"))
        .unwrap_or_else(|| dogfood_status.to_string());
    if let Some(summary) = dogfood.get("summary").filter(|value| !value.is_null()) {
        object_field(Some(summary), "receipt.dogfood.summary")?;
        let accepted = bool_field(summary.get("accepted"), "receipt.dogfood.summary.accepted")?;
        let seed = optional_int(summary.get("seed"), "receipt.dogfood.summary.seed")?;
        let fail_after = optional_int(
            summary.get("snapshot_probe_fail_after"),
            "receipt.dogfood.summary.snapshot_probe_fail_after",
        )?;
        let (replay_class, depth) =
            if let Some(verdict) = summary.get("verdict").filter(|value| !value.is_null()) {
                object_field(Some(verdict), "receipt.dogfood.summary.verdict")?;
                (
                    token_field(
                        verdict.get("replay_class"),
                        "receipt.dogfood.summary.verdict.replay_class",
                    )?
                    .to_string(),
                    optional_int(
                        verdict.get("replay_parent_depth"),
                        "receipt.dogfood.summary.verdict.replay_parent_depth",
                    )?,
                )
            } else {
                ("none".to_string(), None)
            };
        dogfood_label.push_str(&format!(
            ":accepted={}:seed={}:fail_after={}:class={}:depth={}",
            if accepted { "true" } else { "false" },
            seed.map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            fail_after
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            replay_class,
            depth
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string())
        ));
    }
    let failed_label = failed_phase.unwrap_or("none");
    let failed_gates_label = if failed_gates.is_empty() {
        "none".to_string()
    } else {
        failed_gates.join(",")
    };
    Ok(format!(
        "replay-readiness status={status} exit={exit_code} static_gates={passed_gates}/{} failed_gates={failed_gates_label} dogfood={dogfood_label} failed_phase={failed_label} scope={scope_token}",
        gates.len()
    ))
}

fn str_field<'a>(
    value: Option<&'a ::serde_json::Value>,
    field: &str,
) -> crate::EvidenceResult<&'a str> {
    value
        .and_then(::serde_json::Value::as_str)
        .ok_or_else(|| crate::EvidenceError::new(format!("{field}: expected string")))
}

fn token_field<'a>(
    value: Option<&'a ::serde_json::Value>,
    field: &str,
) -> crate::EvidenceResult<&'a str> {
    let token = str_field(value, field)?;
    crate::ensure(
        !token.trim().is_empty(),
        format!("{field}: expected non-empty token"),
    )?;
    Ok(token)
}

fn optional_token<'a>(
    value: Option<&'a ::serde_json::Value>,
    field: &str,
) -> crate::EvidenceResult<Option<&'a str>> {
    match value {
        None | Some(::serde_json::Value::Null) => Ok(None),
        Some(value) => token_field(Some(value), field).map(Some),
    }
}

fn object_field<'a>(
    value: Option<&'a ::serde_json::Value>,
    field: &str,
) -> crate::EvidenceResult<&'a ::serde_json::Map<String, ::serde_json::Value>> {
    value
        .and_then(::serde_json::Value::as_object)
        .ok_or_else(|| crate::EvidenceError::new(format!("{field}: expected object")))
}

fn array_field<'a>(
    value: Option<&'a ::serde_json::Value>,
    field: &str,
) -> crate::EvidenceResult<&'a Vec<::serde_json::Value>> {
    value
        .and_then(::serde_json::Value::as_array)
        .ok_or_else(|| crate::EvidenceError::new(format!("{field}: expected array")))
}

fn int_field(value: Option<&::serde_json::Value>, field: &str) -> crate::EvidenceResult<i64> {
    value
        .and_then(::serde_json::Value::as_i64)
        .ok_or_else(|| crate::EvidenceError::new(format!("{field}: expected integer")))
}

fn optional_int(
    value: Option<&::serde_json::Value>,
    field: &str,
) -> crate::EvidenceResult<Option<i64>> {
    match value {
        None | Some(::serde_json::Value::Null) => Ok(None),
        Some(value) => int_field(Some(value), field).map(Some),
    }
}

fn bool_field(value: Option<&::serde_json::Value>, field: &str) -> crate::EvidenceResult<bool> {
    value
        .and_then(::serde_json::Value::as_bool)
        .ok_or_else(|| crate::EvidenceError::new(format!("{field}: expected boolean")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn valid_in_memory_receipt_is_classified() {
        let receipt = json!({
            "command": "replay-readiness",
            "status": "passed",
            "static_gates": [{"name": "unit", "status": "pass"}],
            "dogfood": {"selected_workload": null, "status": "skipped", "summary": null},
            "failed_phase": null,
            "exit_code": 0,
            "scope": "bounded and not universal"
        });
        let summary = summarize_receipt(&receipt).expect("classify valid receipt");
        assert!(summary.contains("status=passed"));
        assert!(summary.contains("scope=bounded"));
    }

    #[test]
    fn unsupported_status_is_rejected_without_io() {
        let receipt = json!({
            "command": "replay-readiness",
            "status": "unknown",
            "static_gates": [{"name": "unit", "status": "pass"}],
            "dogfood": {"status": "skipped"},
            "exit_code": 0,
            "scope": "bounded and not universal"
        });
        let error = summarize_receipt(&receipt).expect_err("reject unsupported status");
        assert!(error.to_string().contains("unsupported value"));
    }
}
