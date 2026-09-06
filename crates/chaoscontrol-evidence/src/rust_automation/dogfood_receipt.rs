//! Pure legacy dogfood run-config and receipt materialization.

// r[impl chaoscontrol.rust_automation.evidence]
// r[impl chaoscontrol.rust_automation.parity]

use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactFact {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializeInput {
    pub output_name: String,
    pub output_path: String,
    pub git_revision: String,
    pub replay_status: String,
    pub replay_message: String,
    pub replay_exit_status: i64,
    pub replay_command: Option<String>,
    pub checkpoint: ::serde_json::Value,
    pub assertions: ::serde_json::Value,
    pub bugs: Vec<::serde_json::Value>,
    pub bug_paths: Vec<String>,
    pub artifacts: Vec<ArtifactFact>,
    pub run_config_digest: String,
    pub checkpoint_digest: String,
}

pub fn build_run_config(
    output_name: &str,
    checkpoint: &::serde_json::Value,
) -> Result<::serde_json::Value, String> {
    let config = checkpoint
        .get("config")
        .and_then(::serde_json::Value::as_object)
        .ok_or_else(|| String::from("checkpoint.config must be an object"))?;
    let mode = if config
        .get("schedule_diversity")
        .and_then(::serde_json::Value::as_bool)
        == Some(false)
    {
        "hybrid"
    } else {
        "fault-schedule"
    };
    Ok(json!({
        "schema_version": "1",
        "profile": output_name,
        "mode": mode,
        "num_vms": required(config, "num_vms")?,
        "kernel_path": required(config, "kernel_path")?,
        "initrd_path": optional_text(config.get("initrd_path"), "none"),
        "seed": required(config, "seed")?,
        "branch_factor": required(config, "branch_factor")?,
        "ticks_per_branch": required(config, "ticks_per_branch")?,
        "max_rounds": required(config, "max_rounds")?,
        "max_frontier": required(config, "max_frontier")?,
        "quantum": required(config, "quantum")?,
        "coverage_gpa": required(config, "coverage_gpa")?,
        "bootstrap_budget": required(config, "bootstrap_budget")?,
        "raw_log_policy": "debug-only-excluded-from-git",
    }))
}

pub fn encode_run_config_compat(config: &::serde_json::Value) -> Result<Vec<u8>, String> {
    const FIELDS: [&str; 14] = [
        "schema_version",
        "profile",
        "mode",
        "num_vms",
        "kernel_path",
        "initrd_path",
        "seed",
        "branch_factor",
        "ticks_per_branch",
        "max_rounds",
        "max_frontier",
        "quantum",
        "coverage_gpa",
        "bootstrap_budget",
    ];
    let object = config
        .as_object()
        .ok_or_else(|| String::from("run config must be an object"))?;
    let mut lines = Vec::with_capacity(FIELDS.len() + 3);
    lines.push(String::from("{"));
    for field in FIELDS {
        let value = object
            .get(field)
            .ok_or_else(|| format!("run config lacks {field}"))?;
        lines.push(format!(
            "  {}: {},",
            serde_json::to_string(field).map_err(|error| error.to_string())?,
            serde_json::to_string(value).map_err(|error| error.to_string())?,
        ));
    }
    let policy = object
        .get("raw_log_policy")
        .ok_or_else(|| String::from("run config lacks raw_log_policy"))?;
    lines.push(format!(
        "  {}: {}",
        serde_json::to_string("raw_log_policy").map_err(|error| error.to_string())?,
        serde_json::to_string(policy).map_err(|error| error.to_string())?
    ));
    lines.push(String::from("}"));
    Ok(format!("{}\n", lines.join("\n")).into_bytes())
}

pub fn build_receipt(input: &MaterializeInput) -> Result<::serde_json::Value, String> {
    if input.bugs.len() != input.bug_paths.len() {
        return Err(String::from("bug values and paths differ in length"));
    }
    let config = input
        .checkpoint
        .get("config")
        .and_then(::serde_json::Value::as_object)
        .ok_or_else(|| String::from("checkpoint.config must be an object"))?;
    let assertions = input
        .assertions
        .as_array()
        .ok_or_else(|| String::from("assertions must be a list"))?;
    let coverage = json!({
        "registered": assertions.len(),
        "exercised": assertions.iter().filter(|item| item.get("verdict").and_then(::serde_json::Value::as_str) != Some("unexercised")).count(),
        "passed": assertions.iter().filter(|item| item.get("verdict").and_then(::serde_json::Value::as_str) == Some("passed")).count(),
        "failed": assertions.iter().filter(|item| item.get("verdict").and_then(::serde_json::Value::as_str) == Some("failed")).count(),
        "unexercised": assertions.iter().filter(|item| item.get("verdict").and_then(::serde_json::Value::as_str) == Some("unexercised")).count(),
        "summary_path": format!("{}/assertions.json", input.output_path),
    });
    let ticks = required_i64(config, "ticks_per_branch")?;
    let replay_ticks = ticks
        .checked_mul(5)
        .ok_or_else(|| String::from("replay ticks overflow"))?;
    let mut bug_reports = Vec::new();
    for (bug, bug_path) in input.bugs.iter().zip(&input.bug_paths) {
        let depth = bug
            .get("replay_parent_depth")
            .and_then(::serde_json::Value::as_i64)
            .unwrap_or(0);
        let mut context = if depth > 0 {
            String::from("parent-snapshot-required")
        } else {
            String::from("schedule-only-replay")
        };
        if input.replay_status == "known-gap" {
            context.push_str("-insufficient");
        }
        let command = input.replay_command.clone().unwrap_or_else(|| {
            format!(
                "nix run .#explore -- reproduce --kernel {} --initrd {} --bug {} --vms {} --ticks {}",
                display(config.get("kernel_path")),
                optional_text(config.get("initrd_path"), "none"),
                bug_path,
                display(config.get("num_vms")),
                replay_ticks,
            )
        });
        let mut report = json!({
            "path": bug_path,
            "assertion_id": bug.get("assertion_id").cloned().unwrap_or(::serde_json::Value::Null),
            "tick": bug.get("tick").cloned().unwrap_or(::serde_json::Value::Null),
            "replay_parent_depth": depth,
            "replay_context": context,
            "replay_status": input.replay_status,
            "replay_attempt": {
                "command": command,
                "exit_status": input.replay_exit_status,
                "message": input.replay_message,
            }
        });
        if let Some(reference) = bug.get("replay_parent_snapshot_ref") {
            report["replay_parent_snapshot_ref"] = reference.clone();
        }
        bug_reports.push(report);
    }
    let artifact_hashes = input
        .artifacts
        .iter()
        .map(|fact| json!({"path": fact.path, "sha256": fact.sha256}))
        .collect::<Vec<_>>();
    Ok(json!({
        "schema_version": "1",
        "status": input.replay_status,
        "acceptance_status": input.replay_status,
        "git_revision": input.git_revision,
        "run_id": input.output_name,
        "command": format!("nix run .#explore-raft -- --output {}", input.output_path),
        "config": {"path": format!("{}/run-config.json", input.output_path), "digest": input.run_config_digest},
        "kernel_path": required(config, "kernel_path")?,
        "initrd_path": optional_text(config.get("initrd_path"), "none"),
        "artifact_hashes": artifact_hashes,
        "assertion_coverage": coverage,
        "bug_reports": bug_reports,
        "checkpoint_reference": {
            "path": format!("{}/checkpoint.json", input.output_path),
            "digest": input.checkpoint_digest,
            "kernel_path": required(config, "kernel_path")?,
            "initrd_path": optional_text(config.get("initrd_path"), "none"),
            "seed": required(config, "seed")?,
        },
        "raw_logs": [
            {"path": format!("{}/run.log", input.output_path), "policy": "debug-only-excluded-from-git"},
            {"path": format!("{}/reproduce.log", input.output_path), "policy": "debug-only-excluded-from-git"},
        ],
    }))
}

fn required(
    object: &serde_json::Map<String, ::serde_json::Value>,
    field: &str,
) -> Result<::serde_json::Value, String> {
    object
        .get(field)
        .cloned()
        .ok_or_else(|| format!("checkpoint.config.{field} is required"))
}

fn required_i64(
    object: &serde_json::Map<String, ::serde_json::Value>,
    field: &str,
) -> Result<i64, String> {
    object
        .get(field)
        .and_then(::serde_json::Value::as_i64)
        .ok_or_else(|| format!("checkpoint.config.{field} must be an integer"))
}

fn optional_text(value: Option<&::serde_json::Value>, fallback: &str) -> String {
    value
        .and_then(::serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn display(value: Option<&::serde_json::Value>) -> String {
    match value {
        Some(::serde_json::Value::String(value)) => value.clone(),
        Some(value) => value.to_string(),
        None => String::from("null"),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{build_receipt, build_run_config, encode_run_config_compat, MaterializeInput};

    fn checkpoint() -> serde_json::Value {
        json!({"config": {
            "schedule_diversity": false, "num_vms": 3, "kernel_path": "/kernel",
            "initrd_path": null, "seed": 42, "branch_factor": 2, "ticks_per_branch": 80,
            "max_rounds": 3, "max_frontier": 8, "quantum": 100, "coverage_gpa": 4096,
            "bootstrap_budget": 10000
        }})
    }

    #[test]
    fn config_and_receipt_match_legacy_schema() {
        let checkpoint = checkpoint();
        let config = build_run_config("run", &checkpoint).expect("config");
        assert_eq!(config["mode"], "hybrid");
        let encoded =
            String::from_utf8(encode_run_config_compat(&config).expect("encoding")).expect("UTF-8");
        assert!(encoded.starts_with("{\n  \"schema_version\": \"1\",\n  \"profile\": \"run\","));
        assert!(encoded.ends_with("  \"raw_log_policy\": \"debug-only-excluded-from-git\"\n}\n"));
        let input = MaterializeInput {
            output_name: String::from("run"),
            output_path: String::from("/run"),
            git_revision: String::from("rev"),
            replay_status: String::from("known-gap"),
            replay_message: String::from("not reproduced"),
            replay_exit_status: 1,
            replay_command: None,
            checkpoint,
            assertions: json!([{"verdict": "passed"}]),
            bugs: vec![json!({"assertion_id": 7, "tick": 9, "replay_parent_depth": 1})],
            bug_paths: vec![String::from("/run/bug_1.json")],
            artifacts: vec![],
            run_config_digest: String::from("sha256:a"),
            checkpoint_digest: String::from("sha256:b"),
        };
        let receipt = build_receipt(&input).expect("receipt");
        assert_eq!(receipt["assertion_coverage"]["passed"], 1);
        assert_eq!(
            receipt["bug_reports"][0]["replay_context"],
            "parent-snapshot-required-insufficient"
        );
    }

    #[test]
    fn malformed_checkpoint_and_bug_cardinality_fail() {
        assert!(build_run_config("run", &json!({})).is_err());
        let mut input = MaterializeInput {
            output_name: String::from("run"),
            output_path: String::from("/run"),
            git_revision: String::from("rev"),
            replay_status: String::from("accepted"),
            replay_message: String::new(),
            replay_exit_status: 0,
            replay_command: None,
            checkpoint: checkpoint(),
            assertions: json!([]),
            bugs: vec![json!({})],
            bug_paths: vec![],
            artifacts: vec![],
            run_config_digest: String::new(),
            checkpoint_digest: String::new(),
        };
        assert!(build_receipt(&input)
            .expect_err("cardinality")
            .contains("differ"));
        input.bugs.clear();
    }
}
