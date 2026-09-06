//! Pure accepted-verdict dogfood summary projection.

// r[impl chaoscontrol.rust_automation.evidence]
// r[impl chaoscontrol.rust_automation.parity]

use serde_json::json;

pub fn summarize_values(
    output: &std::path::Path,
    accepted: Option<&::serde_json::Value>,
    attempts: Option<&::serde_json::Value>,
) -> Result<::serde_json::Value, String> {
    let canonical_output = output.display().to_string();
    if let Some(summary) = accepted {
        require_object(summary, "accepted summary")?;
        if summary
            .get("accepted")
            .and_then(::serde_json::Value::as_bool)
            != Some(true)
        {
            return Err(String::from("accepted must be true"));
        }
        let mut result = compact_attempt(summary)?;
        insert(&mut result, "accepted", ::serde_json::Value::Bool(true))?;
        insert(
            &mut result,
            "output",
            ::serde_json::Value::String(canonical_output),
        )?;
        insert(
            &mut result,
            "accepted_bug",
            basename_or_null(summary.get("accepted_bug")),
        )?;
        insert(
            &mut result,
            "accepted_verdict",
            basename_or_null(summary.get("accepted_verdict")),
        )?;
        return Ok(result);
    }
    if let Some(summary) = attempts {
        require_object(summary, "attempts summary")?;
        let values = summary
            .get("attempts")
            .and_then(::serde_json::Value::as_array)
            .ok_or_else(|| String::from("attempts must be a list"))?;
        let last = values
            .last()
            .ok_or_else(|| String::from("attempts must not be empty"))?;
        require_object(last, "last attempt")?;
        let mut result = compact_attempt(last)?;
        insert(&mut result, "accepted", ::serde_json::Value::Bool(false))?;
        insert(
            &mut result,
            "output",
            ::serde_json::Value::String(canonical_output),
        )?;
        insert(&mut result, "attempts", json!(values.len()))?;
        return Ok(result);
    }
    Err(String::from(
        "missing accepted-snapshot-verdict-summary.json or attempts-summary.json",
    ))
}

pub fn format_line(summary: &::serde_json::Value) -> String {
    let verdict = summary
        .get("verdict")
        .and_then(::serde_json::Value::as_object);
    let accepted = if summary.get("accepted") == Some(&::serde_json::Value::Bool(true)) {
        "true"
    } else {
        "false"
    };
    let mut parts = vec![
        String::from("dogfood-summary"),
        pair("workload", text_or(summary.get("workload"), "unknown")),
        pair("accepted", accepted.to_string()),
        pair("seed", scalar_or(summary.get("seed"), "unknown")),
        pair(
            "fail_after",
            scalar_or(summary.get("snapshot_probe_fail_after"), "unknown"),
        ),
        pair("run", scalar_or(summary.get("run_exit_status"), "unknown")),
        pair(
            "export",
            scalar_or(summary.get("export_exit_status"), "unknown"),
        ),
        pair(
            "reproduce",
            scalar_or(summary.get("reproduce_exit_status"), "unknown"),
        ),
        pair(
            "class",
            text_or(verdict.and_then(|value| value.get("replay_class")), "none"),
        ),
        pair(
            "depth",
            scalar_or(
                verdict.and_then(|value| value.get("replay_parent_depth")),
                "none",
            ),
        ),
        pair("output", text_or(summary.get("output"), "unknown")),
    ];
    if summary
        .get("attempts")
        .is_some_and(|value| !value.is_null())
    {
        parts.insert(
            4,
            pair("attempts", scalar_or(summary.get("attempts"), "unknown")),
        );
    }
    parts.join(" ")
}

fn compact_attempt(attempt: &::serde_json::Value) -> Result<::serde_json::Value, String> {
    let object = require_object(attempt, "attempt")?;
    let bug_count = match object.get("bugs") {
        None | Some(::serde_json::Value::Null) => ::serde_json::Value::Null,
        Some(::serde_json::Value::Array(items)) => json!(items.len()),
        Some(_) => return Err(String::from("attempt.bugs: expected list or null")),
    };
    let verdict = match object.get("verdict") {
        None | Some(::serde_json::Value::Null) => ::serde_json::Value::Null,
        Some(value) => {
            let verdict = require_object(value, "attempt.verdict")?;
            let reproduced =
                optional_bool(verdict.get("reproduced"), "attempt.verdict.reproduced")?;
            let snapshot_status = optional_token(
                verdict.get("snapshot_status"),
                "attempt.verdict.snapshot_status",
            )?;
            json!({
                "replay_class": require_token(verdict.get("replay_class"), "attempt.verdict.replay_class")?,
                "reproduced": reproduced,
                "replay_parent_depth": optional_integer(verdict.get("replay_parent_depth"), "attempt.verdict.replay_parent_depth")?,
                "snapshot_status": snapshot_status,
            })
        }
    };
    Ok(json!({
        "workload": require_token(object.get("workload"), "attempt.workload")?,
        "seed": optional_integer(object.get("seed"), "attempt.seed")?,
        "snapshot_probe_fail_after": optional_integer(object.get("snapshot_probe_fail_after"), "attempt.snapshot_probe_fail_after")?,
        "run_exit_status": optional_integer(object.get("run_exit_status"), "attempt.run_exit_status")?,
        "export_exit_status": optional_integer(object.get("export_exit_status"), "attempt.export_exit_status")?,
        "reproduce_exit_status": optional_integer(object.get("reproduce_exit_status"), "attempt.reproduce_exit_status")?,
        "bug_count": bug_count,
        "verdict": verdict,
    }))
}

fn require_object<'a>(
    value: &'a ::serde_json::Value,
    field: &str,
) -> Result<&'a ::serde_json::Map<String, ::serde_json::Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{field}: expected object"))
}

fn optional_integer(
    value: Option<&::serde_json::Value>,
    field: &str,
) -> Result<::serde_json::Value, String> {
    match value {
        None | Some(::serde_json::Value::Null) => Ok(::serde_json::Value::Null),
        Some(::serde_json::Value::Number(number)) if number.as_i64().is_some() => {
            Ok(::serde_json::Value::Number(number.clone()))
        }
        _ => Err(format!("{field}: expected integer or null")),
    }
}

fn optional_bool(
    value: Option<&::serde_json::Value>,
    field: &str,
) -> Result<::serde_json::Value, String> {
    match value {
        None | Some(::serde_json::Value::Null) => Ok(::serde_json::Value::Null),
        Some(::serde_json::Value::Bool(value)) => Ok(::serde_json::Value::Bool(*value)),
        _ => Err(format!("{field}: expected boolean or null")),
    }
}

fn require_token(value: Option<&::serde_json::Value>, field: &str) -> Result<String, String> {
    let text = value
        .and_then(::serde_json::Value::as_str)
        .filter(|text| !text.is_empty() && !text.chars().any(char::is_whitespace))
        .ok_or_else(|| format!("{field}: expected non-empty whitespace-free string"))?;
    Ok(text.to_string())
}

fn optional_token(
    value: Option<&::serde_json::Value>,
    field: &str,
) -> Result<::serde_json::Value, String> {
    match value {
        None | Some(::serde_json::Value::Null) => Ok(::serde_json::Value::Null),
        Some(value) => Ok(::serde_json::Value::String(require_token(
            Some(value),
            field,
        )?)),
    }
}

fn basename_or_null(value: Option<&::serde_json::Value>) -> ::serde_json::Value {
    let Some(text) = value.and_then(::serde_json::Value::as_str) else {
        return ::serde_json::Value::Null;
    };
    std::path::Path::new(text)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(|name| ::serde_json::Value::String(name.to_string()))
        .unwrap_or(::serde_json::Value::Null)
}

fn insert(
    target: &mut ::serde_json::Value,
    key: &str,
    value: ::serde_json::Value,
) -> Result<(), String> {
    let object = target
        .as_object_mut()
        .ok_or_else(|| String::from("summary projection is not an object"))?;
    object.insert(key.to_string(), value);
    Ok(())
}

fn scalar_or(value: Option<&::serde_json::Value>, fallback: &str) -> String {
    match value {
        Some(::serde_json::Value::Number(value)) => value.to_string(),
        Some(::serde_json::Value::String(value)) if !value.is_empty() => value.clone(),
        _ => fallback.to_string(),
    }
}

fn text_or(value: Option<&::serde_json::Value>, fallback: &str) -> String {
    value
        .and_then(::serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn pair(name: &str, value: String) -> String {
    format!("{name}={value}")
}

#[cfg(test)]
mod tests {

    use serde_json::json;

    use super::{format_line, summarize_values};

    #[test]
    fn accepted_and_attempt_summaries_preserve_shape() {
        let accepted = json!({
            "accepted": true,
            "workload": "raft",
            "seed": 42,
            "snapshot_probe_fail_after": 1,
            "run_exit_status": 1,
            "export_exit_status": 0,
            "reproduce_exit_status": 0,
            "bugs": [{}],
            "verdict": {"replay_class": "snapshot_backed_reproduced", "reproduced": true, "replay_parent_depth": 1, "snapshot_status": "valid"},
            "accepted_bug": "path/bug_1.json",
            "accepted_verdict": "path/verdict.json"
        });
        let summary = summarize_values(std::path::Path::new("/tmp/output"), Some(&accepted), None)
            .expect("accepted");
        assert_eq!(summary["accepted_bug"], "bug_1.json");
        assert!(format_line(&summary).contains("accepted=true"));

        let attempts = json!({"attempts": [{
            "workload": "raft", "seed": 43, "snapshot_probe_fail_after": 1,
            "run_exit_status": 2, "export_exit_status": null, "reproduce_exit_status": null,
            "bugs": [], "verdict": null
        }]});
        let summary = summarize_values(std::path::Path::new("/tmp/output"), None, Some(&attempts))
            .expect("attempts");
        assert_eq!(summary["attempts"], 1);
        assert!(format_line(&summary).contains("accepted=false"));
    }

    #[test]
    fn malformed_and_empty_attempts_fail() {
        assert!(summarize_values(
            std::path::Path::new("/tmp/output"),
            None,
            Some(&json!({"attempts": []}))
        )
        .expect_err("empty")
        .contains("must not be empty"));
        assert!(summarize_values(
            std::path::Path::new("/tmp/output"),
            Some(&json!({"accepted": false})),
            None
        )
        .expect_err("not accepted")
        .contains("accepted must be true"));
    }
}
