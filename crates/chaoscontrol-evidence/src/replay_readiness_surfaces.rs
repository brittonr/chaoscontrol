use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::{ensure, EvidenceError, EvidenceResult};

pub const README_START_MARKER: &str = "<!-- replay-readiness-status:start -->";
pub const README_END_MARKER: &str = "<!-- replay-readiness-status:end -->";

pub fn summarize_receipt_path(path: impl AsRef<Path>) -> EvidenceResult<String> {
    summarize_receipt(&load_json(path.as_ref())?)
}

pub fn summarize_receipt(receipt: &Value) -> EvidenceResult<String> {
    let command = str_field(receipt.get("command"), "receipt.command")?;
    ensure(
        command == "replay-readiness",
        format!("receipt.command: expected replay-readiness, got {command:?}"),
    )?;
    let status = str_field(receipt.get("status"), "receipt.status")?;
    ensure(
        matches!(status, "passed" | "failed"),
        format!("receipt.status: unsupported value {status:?}"),
    )?;
    let gates = array_field(receipt.get("static_gates"), "receipt.static_gates")?;
    ensure(
        !gates.is_empty(),
        "receipt.static_gates: expected non-empty list",
    )?;

    let mut passed_gates = 0usize;
    let mut failed_gates = Vec::new();
    for (idx, gate) in gates.iter().enumerate() {
        let name = token_field(
            gate.get("name"),
            &format!("receipt.static_gates[{idx}].name"),
        )?;
        let gate_status = str_field(
            gate.get("status"),
            &format!("receipt.static_gates[{idx}].status"),
        )?;
        match gate_status {
            "pass" => passed_gates += 1,
            "fail" => failed_gates.push(name.to_string()),
            "pending" | "running" => {}
            other => {
                return Err(EvidenceError::new(format!(
                    "receipt.static_gates[{idx}].status: unsupported value {other:?}"
                )))
            }
        }
    }

    let dogfood = object_field(receipt.get("dogfood"), "receipt.dogfood")?;
    let selected = optional_token(
        dogfood.get("selected_workload"),
        "receipt.dogfood.selected_workload",
    )?;
    let dogfood_status = str_field(dogfood.get("status"), "receipt.dogfood.status")?;
    ensure(
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
        .map(|s| format!("{s}:{dogfood_status}"))
        .unwrap_or_else(|| dogfood_status.to_string());
    if let Some(summary) = dogfood.get("summary").filter(|v| !v.is_null()) {
        object_field(Some(summary), "receipt.dogfood.summary")?;
        let accepted = bool_field(summary.get("accepted"), "receipt.dogfood.summary.accepted")?;
        let seed = optional_int(summary.get("seed"), "receipt.dogfood.summary.seed")?;
        let fail_after = optional_int(
            summary.get("snapshot_probe_fail_after"),
            "receipt.dogfood.summary.snapshot_probe_fail_after",
        )?;
        let (replay_class, depth) =
            if let Some(verdict) = summary.get("verdict").filter(|v| !v.is_null()) {
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
            seed.map(|v| v.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            fail_after
                .map(|v| v.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            replay_class,
            depth
                .map(|v| v.to_string())
                .unwrap_or_else(|| "none".to_string())
        ));
    }
    let failed_label = failed_phase.unwrap_or("none");
    let failed_gates_label = if failed_gates.is_empty() {
        "none".to_string()
    } else {
        failed_gates.join(",")
    };
    Ok(format!("replay-readiness status={status} exit={exit_code} static_gates={passed_gates}/{} failed_gates={failed_gates_label} dogfood={dogfood_label} failed_phase={failed_label} scope={scope_token}", gates.len()))
}

pub fn write_dashboard_path(
    receipt_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
) -> EvidenceResult<()> {
    let receipt = load_json(receipt_path.as_ref())?;
    let summary = summarize_receipt(&receipt)?;
    let html = render_dashboard(&receipt, &summary)?;
    let output_path = output_path.as_ref();
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, html)?;
    Ok(())
}

pub fn write_fleet_triage_index_path(
    receipt_paths: &[impl AsRef<Path>],
    output_path: impl AsRef<Path>,
) -> EvidenceResult<()> {
    let html = render_fleet_triage_index_path(receipt_paths)?;
    let output_path = output_path.as_ref();
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, html)?;
    Ok(())
}

pub fn render_fleet_triage_index_path(
    receipt_paths: &[impl AsRef<Path>],
) -> EvidenceResult<String> {
    ensure(
        !receipt_paths.is_empty(),
        "fleet triage index requires at least one replay-readiness receipt",
    )?;
    let mut entries = Vec::with_capacity(receipt_paths.len());
    for path in receipt_paths {
        let path = path.as_ref();
        let receipt = load_json(path)?;
        entries.push((
            path.display().to_string(),
            receipt,
            summarize_receipt_path(path)?,
        ));
    }
    render_fleet_triage_index(&entries)
}

pub fn render_fleet_triage_index(entries: &[(String, Value, String)]) -> EvidenceResult<String> {
    ensure(
        !entries.is_empty(),
        "fleet triage index requires at least one entry",
    )?;
    let mut rows = String::new();
    let mut pass_count = 0usize;
    for (path, receipt, summary) in entries {
        let status = str_field(receipt.get("status"), "receipt.status")?;
        if status == "passed" {
            pass_count += 1;
        }
        let dogfood = object_field(receipt.get("dogfood"), "receipt.dogfood")?;
        let dogfood_summary = dogfood.get("summary").filter(|v| v.is_object());
        let verdict = dogfood_summary
            .and_then(|v| v.get("verdict"))
            .filter(|v| v.is_object());
        rows.push_str(&format!(
            "<tr><td><code>{}</code></td><td><span class=\"pill {}\">{}</span></td><td>{}</td><td>{}</td><td>{}</td><td><code>{}</code></td></tr>\n",
            esc(path),
            token_class(status),
            esc(status),
            esc_value(dogfood.get("selected_workload")),
            esc_value(verdict.and_then(|v| v.get("replay_class"))),
            esc_value(verdict.and_then(|v| v.get("replay_parent_depth"))),
            esc(summary),
        ));
    }
    Ok(format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>ChaosControl fleet triage index</title>
<style>
:root {{ color-scheme: light dark; --ok:#138a36; --bad:#b42318; --warn:#b7791f; --border:#98a2b3; }}
body {{ font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; margin: 2rem; line-height: 1.45; }}
.pill {{ border-radius: 999px; color: white; display: inline-block; font-weight: 700; padding: .15rem .55rem; }}
.ok {{ background: var(--ok); }} .bad {{ background: var(--bad); }} .warn {{ background: var(--warn); }}
table {{ border-collapse: collapse; width: 100%; }} th, td {{ border-bottom: 1px solid var(--border); padding: .55rem; text-align: left; vertical-align: top; }}
code {{ background: rgba(127,127,127,.14); border-radius: .35rem; padding: .1rem .25rem; }}
.scope {{ border-left: .35rem solid var(--warn); padding-left: .8rem; }}
</style>
</head>
<body>
<h1>ChaosControl fleet triage index</h1>
<p><strong>{}</strong> replay-readiness receipts indexed; <strong>{}</strong> passed.</p>
<p class="scope">This is a bounded static multi-receipt triage artifact for fleet-style review. It is not universal replay evidence, not a hosted service, not scheduler integration, not a shared decision store, and not a full Antithesis-style product replacement.</p>
<table><thead><tr><th>Receipt</th><th>Status</th><th>Workload</th><th>Replay class</th><th>Depth</th><th>Summary</th></tr></thead><tbody>
{}</tbody></table>
</body>
</html>
"#,
        entries.len(),
        pass_count,
        rows
    ))
}

pub fn write_decision_receipt_path(output_path: impl AsRef<Path>) -> EvidenceResult<()> {
    let output_path = output_path.as_ref();
    let receipt = sample_decision_receipt();
    validate_decision_receipt(&receipt)?;
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, serde_json::to_vec_pretty(&receipt)?)?;
    Ok(())
}

pub fn validate_decision_receipt_path(path: impl AsRef<Path>) -> EvidenceResult<String> {
    validate_decision_receipt(&load_json(path.as_ref())?)
}

pub fn write_scheduler_receipt_path(output_path: impl AsRef<Path>) -> EvidenceResult<()> {
    let output_path = output_path.as_ref();
    let receipt = sample_scheduler_receipt();
    validate_scheduler_receipt(&receipt)?;
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, serde_json::to_vec_pretty(&receipt)?)?;
    Ok(())
}

pub fn validate_scheduler_receipt_path(path: impl AsRef<Path>) -> EvidenceResult<String> {
    validate_scheduler_receipt(&load_json(path.as_ref())?)
}

pub fn validate_decision_receipt(receipt: &Value) -> EvidenceResult<String> {
    let schema_version = int_field(receipt.get("schema_version"), "decision.schema_version")?;
    ensure(
        schema_version == 1,
        format!("decision.schema_version: expected 1, got {schema_version}"),
    )?;
    let command = str_field(receipt.get("command"), "decision.command")?;
    ensure(
        command == "replay-readiness-decision-receipt",
        format!("decision.command: expected replay-readiness-decision-receipt, got {command:?}"),
    )?;
    let status = str_field(receipt.get("status"), "decision.status")?;
    ensure(
        status == "recorded",
        format!("decision.status: unsupported value {status:?}"),
    )?;
    let scope = str_field(receipt.get("scope"), "decision.scope")?;
    ensure(
        scope.contains("local")
            && scope.contains("bounded")
            && scope.contains("not a shared decision store"),
        "decision.scope: must declare bounded local scope and not a shared decision store",
    )?;
    ensure(
        !matches!(receipt.get("raw_log_scraping"), Some(Value::Bool(true))),
        "decision.raw_log_scraping: raw-log scraping is not allowed",
    )?;

    let source = object_field(receipt.get("source"), "decision.source")?;
    str_field(source.get("fleet_index"), "decision.source.fleet_index")?;
    let receipt_paths = array_field(source.get("receipt_paths"), "decision.source.receipt_paths")?;
    ensure(
        !receipt_paths.is_empty(),
        "decision.source.receipt_paths: expected non-empty list",
    )?;
    for (idx, path) in receipt_paths.iter().enumerate() {
        str_field(Some(path), &format!("decision.source.receipt_paths[{idx}]"))?;
    }

    let decisions = array_field(receipt.get("decisions"), "decision.decisions")?;
    ensure(
        !decisions.is_empty(),
        "decision.decisions: expected non-empty list",
    )?;
    let mut ids = BTreeSet::new();
    let mut actions = BTreeSet::new();
    for (idx, decision) in decisions.iter().enumerate() {
        let decision = object_field(Some(decision), &format!("decision.decisions[{idx}]"))?;
        let id = token_field(
            decision.get("decision_id"),
            &format!("decision.decisions[{idx}].decision_id"),
        )?;
        ensure(
            ids.insert(id.to_string()),
            format!("decision.decisions[{idx}].decision_id: duplicate {id}"),
        )?;
        str_field(
            decision.get("receipt_path"),
            &format!("decision.decisions[{idx}].receipt_path"),
        )?;
        str_field(
            decision.get("operator"),
            &format!("decision.decisions[{idx}].operator"),
        )?;
        let action = token_field(
            decision.get("action"),
            &format!("decision.decisions[{idx}].action"),
        )?;
        ensure(
            matches!(
                action,
                "accept-for-local-review" | "reproduce" | "minimize" | "defer" | "reject"
            ),
            format!("decision.decisions[{idx}].action: unsupported value {action:?}"),
        )?;
        actions.insert(action.to_string());
        str_field(
            decision.get("rationale"),
            &format!("decision.decisions[{idx}].rationale"),
        )?;
        token_field(
            decision.get("recorded_at"),
            &format!("decision.decisions[{idx}].recorded_at"),
        )?;
        if let Some(Value::String(_)) = decision.get("replay_class") {
            token_field(
                decision.get("replay_class"),
                &format!("decision.decisions[{idx}].replay_class"),
            )?;
        }
        let artifacts = array_field(
            decision.get("linked_artifacts"),
            &format!("decision.decisions[{idx}].linked_artifacts"),
        )?;
        for (artifact_idx, artifact) in artifacts.iter().enumerate() {
            str_field(
                Some(artifact),
                &format!("decision.decisions[{idx}].linked_artifacts[{artifact_idx}]"),
            )?;
        }
    }

    let anti_claims = array_field(receipt.get("anti_claims"), "decision.anti_claims")?;
    let anti_claim_text = anti_claims
        .iter()
        .map(json_display)
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();
    ensure(
        anti_claim_text.contains("not a shared decision store")
            && anti_claim_text.contains("not a hosted service")
            && anti_claim_text.contains("no raw-log scraping"),
        "decision.anti_claims: missing local-scope anti-overclaim text",
    )?;

    Ok(format!(
        "replay-readiness-decision-receipt status={status} decisions={} actions={} receipts={} scope=bounded-local-not-shared",
        decisions.len(),
        actions.into_iter().collect::<Vec<_>>().join(","),
        receipt_paths.len()
    ))
}

pub fn validate_scheduler_receipt(receipt: &Value) -> EvidenceResult<String> {
    let schema_version = int_field(receipt.get("schema_version"), "scheduler.schema_version")?;
    ensure(
        schema_version == 1,
        format!("scheduler.schema_version: expected 1, got {schema_version}"),
    )?;
    let command = str_field(receipt.get("command"), "scheduler.command")?;
    ensure(
        command == "replay-readiness-scheduler-receipt",
        format!("scheduler.command: expected replay-readiness-scheduler-receipt, got {command:?}"),
    )?;
    let status = str_field(receipt.get("status"), "scheduler.status")?;
    ensure(
        matches!(status, "planned" | "recorded" | "partial"),
        format!("scheduler.status: unsupported value {status:?}"),
    )?;
    let scope = str_field(receipt.get("scope"), "scheduler.scope")?;
    ensure(
        scope.contains("bounded")
            && scope.contains("local")
            && scope.contains("not a hosted service")
            && scope.contains("not a fleet-scale scheduler"),
        "scheduler.scope: must declare bounded local scope and not a hosted/fleet scheduler",
    )?;
    ensure(
        !matches!(receipt.get("raw_log_scraping"), Some(Value::Bool(true))),
        "scheduler.raw_log_scraping: raw-log scraping is not allowed",
    )?;

    let schedule = object_field(receipt.get("schedule"), "scheduler.schedule")?;
    let mode = token_field(schedule.get("mode"), "scheduler.schedule.mode")?;
    ensure(
        matches!(mode, "manual-batch" | "cron-preview"),
        format!("scheduler.schedule.mode: unsupported value {mode:?}"),
    )?;
    let max_runs = int_field(schedule.get("max_runs"), "scheduler.schedule.max_runs")?;
    let concurrency = int_field(
        schedule.get("concurrency"),
        "scheduler.schedule.concurrency",
    )?;
    ensure(
        max_runs > 0,
        "scheduler.schedule.max_runs: expected positive integer",
    )?;
    ensure(
        concurrency > 0 && concurrency <= max_runs,
        "scheduler.schedule.concurrency: expected positive integer no larger than max_runs",
    )?;

    let run_plan = array_field(receipt.get("run_plan"), "scheduler.run_plan")?;
    ensure(
        !run_plan.is_empty(),
        "scheduler.run_plan: expected non-empty list",
    )?;
    ensure(
        run_plan.len() as i64 <= max_runs,
        "scheduler.run_plan: cannot exceed schedule.max_runs",
    )?;
    let mut run_ids = BTreeSet::new();
    let mut workloads = BTreeSet::new();
    for (idx, run) in run_plan.iter().enumerate() {
        let run = object_field(Some(run), &format!("scheduler.run_plan[{idx}]"))?;
        let run_id = token_field(
            run.get("run_id"),
            &format!("scheduler.run_plan[{idx}].run_id"),
        )?;
        ensure(
            run_ids.insert(run_id.to_string()),
            format!("scheduler.run_plan[{idx}].run_id: duplicate {run_id}"),
        )?;
        let workload = token_field(
            run.get("workload"),
            &format!("scheduler.run_plan[{idx}].workload"),
        )?;
        workloads.insert(workload.to_string());
        str_field(
            run.get("command"),
            &format!("scheduler.run_plan[{idx}].command"),
        )?;
        str_field(
            run.get("receipt_path"),
            &format!("scheduler.run_plan[{idx}].receipt_path"),
        )?;
        let decision_policy = token_field(
            run.get("decision_policy"),
            &format!("scheduler.run_plan[{idx}].decision_policy"),
        )?;
        ensure(
            matches!(decision_policy, "record-local-decision" | "skip-decision"),
            format!(
                "scheduler.run_plan[{idx}].decision_policy: unsupported value {decision_policy:?}"
            ),
        )?;
    }

    let anti_claims = array_field(receipt.get("anti_claims"), "scheduler.anti_claims")?;
    let anti_claim_text = anti_claims
        .iter()
        .map(json_display)
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();
    ensure(
        anti_claim_text.contains("not a hosted service")
            && anti_claim_text.contains("not a fleet-scale scheduler")
            && anti_claim_text.contains("not a shared queue")
            && anti_claim_text.contains("no raw-log scraping"),
        "scheduler.anti_claims: missing scheduler anti-overclaim text",
    )?;

    Ok(format!(
        "replay-readiness-scheduler-receipt status={status} runs={} workloads={} mode={mode} scope=bounded-local-not-hosted",
        run_plan.len(),
        workloads.into_iter().collect::<Vec<_>>().join(",")
    ))
}

pub fn execute_scheduler_receipt_path(
    plan_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
) -> EvidenceResult<String> {
    let plan_path = plan_path.as_ref();
    let output_path = output_path.as_ref();
    let plan = load_json(plan_path)?;
    validate_scheduler_receipt(&plan)?;
    let execution = execute_scheduler_receipt(&plan, plan_path)?;
    let summary = validate_scheduler_execution_receipt(&execution)?;
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, serde_json::to_vec_pretty(&execution)?)?;
    Ok(summary)
}

pub fn validate_scheduler_execution_receipt_path(path: impl AsRef<Path>) -> EvidenceResult<String> {
    validate_scheduler_execution_receipt(&load_json(path.as_ref())?)
}

pub fn execute_scheduler_receipt(plan: &Value, plan_path: &Path) -> EvidenceResult<Value> {
    let schedule = object_field(plan.get("schedule"), "scheduler.schedule")?;
    let concurrency = int_field(
        schedule.get("concurrency"),
        "scheduler.schedule.concurrency",
    )?;
    ensure(
        concurrency == 1,
        "scheduler execution currently supports bounded local sequential concurrency=1 only",
    )?;
    let run_plan = array_field(plan.get("run_plan"), "scheduler.run_plan")?;
    let started_at = unix_seconds();
    let mut runs = Vec::with_capacity(run_plan.len());
    let mut failures = 0usize;
    for (idx, run) in run_plan.iter().enumerate() {
        let run = object_field(Some(run), &format!("scheduler.run_plan[{idx}]"))?;
        let run_id = token_field(
            run.get("run_id"),
            &format!("scheduler.run_plan[{idx}].run_id"),
        )?;
        let workload = token_field(
            run.get("workload"),
            &format!("scheduler.run_plan[{idx}].workload"),
        )?;
        let command = str_field(
            run.get("command"),
            &format!("scheduler.run_plan[{idx}].command"),
        )?;
        let receipt_path = str_field(
            run.get("receipt_path"),
            &format!("scheduler.run_plan[{idx}].receipt_path"),
        )?;
        let decision_policy = token_field(
            run.get("decision_policy"),
            &format!("scheduler.run_plan[{idx}].decision_policy"),
        )?;
        let run_started = unix_seconds();
        let status = Command::new("sh")
            .arg("-lc")
            .arg(command)
            .status()
            .map_err(|err| EvidenceError::new(format!("scheduler run {run_id}: {err}")))?;
        let exit_code = status.code().unwrap_or(125);
        let receipt_summary = if exit_code == 0 {
            Some(summarize_receipt_path(receipt_path)?)
        } else {
            None
        };
        if exit_code != 0 {
            failures += 1;
        }
        runs.push(json!({
            "run_id": run_id,
            "workload": workload,
            "command": command,
            "receipt_path": receipt_path,
            "decision_policy": decision_policy,
            "started_at_unix": run_started,
            "finished_at_unix": unix_seconds(),
            "exit_code": exit_code,
            "status": if exit_code == 0 { "passed" } else { "failed" },
            "receipt_summary": receipt_summary,
        }));
    }
    let status = if failures == 0 {
        "passed"
    } else if failures == runs.len() {
        "failed"
    } else {
        "partial"
    };
    Ok(json!({
        "schema_version": 1,
        "command": "replay-readiness-scheduler-execution",
        "status": status,
        "plan_path": plan_path.display().to_string(),
        "started_at_unix": started_at,
        "finished_at_unix": unix_seconds(),
        "scope": "bounded local sequential scheduler execution receipt; not a hosted service, not a fleet-scale scheduler, not a shared queue, and not product-parity evidence",
        "raw_log_scraping": false,
        "schedule": schedule,
        "runs": runs,
        "anti_claims": [
            "This is not a hosted service.",
            "This is not a fleet-scale scheduler and not a shared queue.",
            "This scheduler execution receipt captures command status and receipt summaries without raw-log scraping."
        ]
    }))
}

pub fn validate_scheduler_execution_receipt(receipt: &Value) -> EvidenceResult<String> {
    let schema_version = int_field(
        receipt.get("schema_version"),
        "scheduler_execution.schema_version",
    )?;
    ensure(
        schema_version == 1,
        format!("scheduler_execution.schema_version: expected 1, got {schema_version}"),
    )?;
    let command = str_field(receipt.get("command"), "scheduler_execution.command")?;
    ensure(
        command == "replay-readiness-scheduler-execution",
        format!("scheduler_execution.command: expected replay-readiness-scheduler-execution, got {command:?}"),
    )?;
    let status = str_field(receipt.get("status"), "scheduler_execution.status")?;
    ensure(
        matches!(status, "passed" | "failed" | "partial"),
        format!("scheduler_execution.status: unsupported value {status:?}"),
    )?;
    let scope = str_field(receipt.get("scope"), "scheduler_execution.scope")?;
    ensure(
        scope.contains("bounded")
            && scope.contains("local")
            && scope.contains("not a hosted service")
            && scope.contains("not a fleet-scale scheduler")
            && scope.contains("not a shared queue"),
        "scheduler_execution.scope: must declare bounded local scope and not hosted/fleet/shared-queue scheduler",
    )?;
    ensure(
        !matches!(receipt.get("raw_log_scraping"), Some(Value::Bool(true))),
        "scheduler_execution.raw_log_scraping: raw-log scraping is not allowed",
    )?;
    let schedule = object_field(receipt.get("schedule"), "scheduler_execution.schedule")?;
    let concurrency = int_field(
        schedule.get("concurrency"),
        "scheduler_execution.schedule.concurrency",
    )?;
    ensure(
        concurrency == 1,
        "scheduler_execution.schedule.concurrency: expected bounded sequential concurrency=1",
    )?;
    let runs = array_field(receipt.get("runs"), "scheduler_execution.runs")?;
    ensure(
        !runs.is_empty(),
        "scheduler_execution.runs: expected non-empty list",
    )?;
    let mut run_ids = BTreeSet::new();
    let mut workloads = BTreeSet::new();
    let mut passed = 0usize;
    for (idx, run) in runs.iter().enumerate() {
        let run = object_field(Some(run), &format!("scheduler_execution.runs[{idx}]"))?;
        let run_id = token_field(
            run.get("run_id"),
            &format!("scheduler_execution.runs[{idx}].run_id"),
        )?;
        ensure(
            run_ids.insert(run_id.to_string()),
            format!("scheduler_execution.runs[{idx}].run_id: duplicate {run_id}"),
        )?;
        let workload = token_field(
            run.get("workload"),
            &format!("scheduler_execution.runs[{idx}].workload"),
        )?;
        workloads.insert(workload.to_string());
        str_field(
            run.get("command"),
            &format!("scheduler_execution.runs[{idx}].command"),
        )?;
        str_field(
            run.get("receipt_path"),
            &format!("scheduler_execution.runs[{idx}].receipt_path"),
        )?;
        let run_status = str_field(
            run.get("status"),
            &format!("scheduler_execution.runs[{idx}].status"),
        )?;
        ensure(
            matches!(run_status, "passed" | "failed"),
            format!("scheduler_execution.runs[{idx}].status: unsupported value {run_status:?}"),
        )?;
        let exit_code = int_field(
            run.get("exit_code"),
            &format!("scheduler_execution.runs[{idx}].exit_code"),
        )?;
        if run_status == "passed" {
            ensure(
                exit_code == 0,
                format!("scheduler_execution.runs[{idx}].exit_code: passed run must exit 0"),
            )?;
            str_field(
                run.get("receipt_summary"),
                &format!("scheduler_execution.runs[{idx}].receipt_summary"),
            )?;
            passed += 1;
        } else {
            ensure(
                exit_code != 0,
                format!("scheduler_execution.runs[{idx}].exit_code: failed run must be nonzero"),
            )?;
        }
    }
    let anti_claims = array_field(
        receipt.get("anti_claims"),
        "scheduler_execution.anti_claims",
    )?;
    let anti_claim_text = anti_claims
        .iter()
        .map(json_display)
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();
    ensure(
        anti_claim_text.contains("not a hosted service")
            && anti_claim_text.contains("not a fleet-scale scheduler")
            && anti_claim_text.contains("not a shared queue")
            && anti_claim_text.contains("without raw-log scraping"),
        "scheduler_execution.anti_claims: missing scheduler execution anti-overclaim text",
    )?;
    Ok(format!(
        "replay-readiness-scheduler-execution status={status} runs={} passed={} workloads={} scope=bounded-local-sequential-not-hosted",
        runs.len(),
        passed,
        workloads.into_iter().collect::<Vec<_>>().join(",")
    ))
}

pub fn write_fleet_scheduler_receipt_path(path: impl AsRef<Path>) -> EvidenceResult<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&sample_fleet_scheduler_receipt())?,
    )?;
    Ok(())
}

pub fn validate_fleet_scheduler_receipt_path(path: impl AsRef<Path>) -> EvidenceResult<String> {
    validate_fleet_scheduler_receipt(&load_json(path.as_ref())?)
}

pub fn sample_fleet_scheduler_receipt() -> Value {
    json!({
        "schema_version": 1,
        "command": "replay-readiness-fleet-scheduler-receipt",
        "status": "recorded",
        "generated_at": "2026-05-11T00:00:00Z",
        "scope": "bounded hosted/fleet scheduler receipt with durable queue leases and worker run receipts; not product-parity evidence, not a full Antithesis replacement, and not raw-log evidence",
        "raw_log_scraping": false,
        "queue": {
            "kind": "durable-file-backed",
            "queue_id": "fleet-queue-0001",
            "lease_timeout_seconds": 900,
            "max_concurrency": 2,
            "entries": [
                {"queue_entry_id": "queue-raft-0001", "run_id": "fleet-run-raft-0001", "workload": "raft", "state": "completed"},
                {"queue_entry_id": "queue-redb-0001", "run_id": "fleet-run-redb-0001", "workload": "redb", "state": "completed"}
            ]
        },
        "workers": [
            {"worker_id": "worker-a", "node_id": "node-a", "lease_id": "lease-raft-0001", "status": "idle"},
            {"worker_id": "worker-b", "node_id": "node-b", "lease_id": "lease-redb-0001", "status": "idle"}
        ],
        "runs": [
            {
                "run_id": "fleet-run-raft-0001",
                "queue_entry_id": "queue-raft-0001",
                "worker_id": "worker-a",
                "workload": "raft",
                "receipt_path": "target/fleet/raft-replay-readiness.json",
                "receipt_summary": "replay-readiness status=passed dogfood=raft:pass scope=bounded",
                "status": "passed",
                "exit_code": 0
            },
            {
                "run_id": "fleet-run-redb-0001",
                "queue_entry_id": "queue-redb-0001",
                "worker_id": "worker-b",
                "workload": "redb",
                "receipt_path": "target/fleet/redb-replay-readiness.json",
                "receipt_summary": "replay-readiness status=passed dogfood=redb:pass scope=bounded",
                "status": "passed",
                "exit_code": 0
            }
        ],
        "operator_decisions": ["target/decision-receipt.json"],
        "anti_claims": [
            "This is bounded hosted/fleet scheduler evidence, not product parity.",
            "This is not a full Antithesis replacement.",
            "This fleet scheduler receipt captures durable queue, lease, worker, run, and receipt-summary state without raw-log scraping."
        ]
    })
}

pub fn validate_fleet_scheduler_receipt(receipt: &Value) -> EvidenceResult<String> {
    let schema_version = int_field(
        receipt.get("schema_version"),
        "fleet_scheduler.schema_version",
    )?;
    ensure(
        schema_version == 1,
        format!("fleet_scheduler.schema_version: expected 1, got {schema_version}"),
    )?;
    let command = str_field(receipt.get("command"), "fleet_scheduler.command")?;
    ensure(
        command == "replay-readiness-fleet-scheduler-receipt",
        format!("fleet_scheduler.command: expected replay-readiness-fleet-scheduler-receipt, got {command:?}"),
    )?;
    let status = str_field(receipt.get("status"), "fleet_scheduler.status")?;
    ensure(
        matches!(status, "recorded" | "partial" | "failed"),
        format!("fleet_scheduler.status: unsupported value {status:?}"),
    )?;
    let scope = str_field(receipt.get("scope"), "fleet_scheduler.scope")?;
    ensure(
        scope.contains("bounded")
            && scope.contains("hosted/fleet")
            && scope.contains("durable queue")
            && scope.contains("worker")
            && scope.contains("not product-parity"),
        "fleet_scheduler.scope: must declare bounded hosted/fleet durable queue and no product-parity claim",
    )?;
    ensure(
        !matches!(receipt.get("raw_log_scraping"), Some(Value::Bool(true))),
        "fleet_scheduler.raw_log_scraping: raw-log scraping is not allowed",
    )?;

    let queue = object_field(receipt.get("queue"), "fleet_scheduler.queue")?;
    let queue_kind = token_field(queue.get("kind"), "fleet_scheduler.queue.kind")?;
    ensure(
        matches!(queue_kind, "durable-file-backed" | "durable-service-backed"),
        format!("fleet_scheduler.queue.kind: unsupported value {queue_kind:?}"),
    )?;
    token_field(queue.get("queue_id"), "fleet_scheduler.queue.queue_id")?;
    let lease_timeout_seconds = int_field(
        queue.get("lease_timeout_seconds"),
        "fleet_scheduler.queue.lease_timeout_seconds",
    )?;
    ensure(
        lease_timeout_seconds > 0,
        "fleet_scheduler.queue.lease_timeout_seconds: expected positive integer",
    )?;
    let max_concurrency = int_field(
        queue.get("max_concurrency"),
        "fleet_scheduler.queue.max_concurrency",
    )?;
    ensure(
        max_concurrency > 0,
        "fleet_scheduler.queue.max_concurrency: expected positive integer",
    )?;
    let entries = array_field(queue.get("entries"), "fleet_scheduler.queue.entries")?;
    ensure(
        !entries.is_empty(),
        "fleet_scheduler.queue.entries: expected non-empty list",
    )?;
    let mut entry_ids = BTreeSet::new();
    let mut entry_run_ids = BTreeSet::new();
    for (idx, entry) in entries.iter().enumerate() {
        let entry = object_field(
            Some(entry),
            &format!("fleet_scheduler.queue.entries[{idx}]"),
        )?;
        let entry_id = token_field(
            entry.get("queue_entry_id"),
            &format!("fleet_scheduler.queue.entries[{idx}].queue_entry_id"),
        )?;
        ensure(
            entry_ids.insert(entry_id.to_string()),
            format!("fleet_scheduler.queue.entries[{idx}].queue_entry_id: duplicate {entry_id}"),
        )?;
        let run_id = token_field(
            entry.get("run_id"),
            &format!("fleet_scheduler.queue.entries[{idx}].run_id"),
        )?;
        entry_run_ids.insert(run_id.to_string());
        token_field(
            entry.get("workload"),
            &format!("fleet_scheduler.queue.entries[{idx}].workload"),
        )?;
        let state = token_field(
            entry.get("state"),
            &format!("fleet_scheduler.queue.entries[{idx}].state"),
        )?;
        ensure(
            matches!(state, "queued" | "leased" | "completed" | "failed"),
            format!("fleet_scheduler.queue.entries[{idx}].state: unsupported value {state:?}"),
        )?;
    }

    let workers = array_field(receipt.get("workers"), "fleet_scheduler.workers")?;
    ensure(
        !workers.is_empty(),
        "fleet_scheduler.workers: expected non-empty list",
    )?;
    let mut worker_ids = BTreeSet::new();
    for (idx, worker) in workers.iter().enumerate() {
        let worker = object_field(Some(worker), &format!("fleet_scheduler.workers[{idx}]"))?;
        let worker_id = token_field(
            worker.get("worker_id"),
            &format!("fleet_scheduler.workers[{idx}].worker_id"),
        )?;
        ensure(
            worker_ids.insert(worker_id.to_string()),
            format!("fleet_scheduler.workers[{idx}].worker_id: duplicate {worker_id}"),
        )?;
        token_field(
            worker.get("node_id"),
            &format!("fleet_scheduler.workers[{idx}].node_id"),
        )?;
        token_field(
            worker.get("lease_id"),
            &format!("fleet_scheduler.workers[{idx}].lease_id"),
        )?;
        let worker_status = token_field(
            worker.get("status"),
            &format!("fleet_scheduler.workers[{idx}].status"),
        )?;
        ensure(
            matches!(worker_status, "idle" | "running" | "offline"),
            format!("fleet_scheduler.workers[{idx}].status: unsupported value {worker_status:?}"),
        )?;
    }

    let runs = array_field(receipt.get("runs"), "fleet_scheduler.runs")?;
    ensure(
        !runs.is_empty(),
        "fleet_scheduler.runs: expected non-empty list",
    )?;
    let mut run_ids = BTreeSet::new();
    let mut workloads = BTreeSet::new();
    let mut passed = 0usize;
    for (idx, run) in runs.iter().enumerate() {
        let run = object_field(Some(run), &format!("fleet_scheduler.runs[{idx}]"))?;
        let run_id = token_field(
            run.get("run_id"),
            &format!("fleet_scheduler.runs[{idx}].run_id"),
        )?;
        ensure(
            run_ids.insert(run_id.to_string()),
            format!("fleet_scheduler.runs[{idx}].run_id: duplicate {run_id}"),
        )?;
        ensure(
            entry_run_ids.contains(run_id),
            format!("fleet_scheduler.runs[{idx}].run_id: {run_id} missing from queue entries"),
        )?;
        let queue_entry_id = token_field(
            run.get("queue_entry_id"),
            &format!("fleet_scheduler.runs[{idx}].queue_entry_id"),
        )?;
        ensure(entry_ids.contains(queue_entry_id), format!("fleet_scheduler.runs[{idx}].queue_entry_id: {queue_entry_id} missing from queue entries"))?;
        let worker_id = token_field(
            run.get("worker_id"),
            &format!("fleet_scheduler.runs[{idx}].worker_id"),
        )?;
        ensure(
            worker_ids.contains(worker_id),
            format!("fleet_scheduler.runs[{idx}].worker_id: {worker_id} missing from workers"),
        )?;
        let workload = token_field(
            run.get("workload"),
            &format!("fleet_scheduler.runs[{idx}].workload"),
        )?;
        workloads.insert(workload.to_string());
        str_field(
            run.get("receipt_path"),
            &format!("fleet_scheduler.runs[{idx}].receipt_path"),
        )?;
        let run_status = token_field(
            run.get("status"),
            &format!("fleet_scheduler.runs[{idx}].status"),
        )?;
        ensure(
            matches!(run_status, "passed" | "failed"),
            format!("fleet_scheduler.runs[{idx}].status: unsupported value {run_status:?}"),
        )?;
        let exit_code = int_field(
            run.get("exit_code"),
            &format!("fleet_scheduler.runs[{idx}].exit_code"),
        )?;
        if run_status == "passed" {
            ensure(
                exit_code == 0,
                format!("fleet_scheduler.runs[{idx}].exit_code: passed run must exit 0"),
            )?;
            let summary = str_field(
                run.get("receipt_summary"),
                &format!("fleet_scheduler.runs[{idx}].receipt_summary"),
            )?;
            ensure(summary.contains("replay-readiness status="), format!("fleet_scheduler.runs[{idx}].receipt_summary: expected replay-readiness summary"))?;
            passed += 1;
        } else {
            ensure(
                exit_code != 0,
                format!("fleet_scheduler.runs[{idx}].exit_code: failed run must be nonzero"),
            )?;
        }
    }

    let decisions = array_field(
        receipt.get("operator_decisions"),
        "fleet_scheduler.operator_decisions",
    )?;
    ensure(
        !decisions.is_empty(),
        "fleet_scheduler.operator_decisions: expected at least one linked decision receipt",
    )?;
    for (idx, decision) in decisions.iter().enumerate() {
        str_field(
            Some(decision),
            &format!("fleet_scheduler.operator_decisions[{idx}]"),
        )?;
    }

    let anti_claims = array_field(receipt.get("anti_claims"), "fleet_scheduler.anti_claims")?;
    let anti_claim_text = anti_claims
        .iter()
        .map(json_display)
        .collect::<Vec<_>>()
        .join(
            "
",
        )
        .to_lowercase();
    ensure(
        anti_claim_text.contains("bounded hosted/fleet")
            && anti_claim_text.contains("not product parity")
            && anti_claim_text.contains("not a full antithesis replacement")
            && anti_claim_text.contains("without raw-log scraping"),
        "fleet_scheduler.anti_claims: missing bounded hosted/fleet anti-overclaim text",
    )?;
    Ok(format!(
        "replay-readiness-fleet-scheduler status={status} queue={queue_kind} workers={} runs={} passed={} workloads={} scope=bounded-hosted-fleet",
        workers.len(),
        runs.len(),
        passed,
        workloads.into_iter().collect::<Vec<_>>().join(",")
    ))
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

pub fn render_dashboard(receipt: &Value, summary_line: &str) -> EvidenceResult<String> {
    let status = str_field(receipt.get("status"), "receipt.status")?;
    let gates = array_field(receipt.get("static_gates"), "receipt.static_gates")?;
    ensure(
        !gates.is_empty(),
        "receipt.static_gates: expected non-empty list",
    )?;
    let dogfood = object_field(receipt.get("dogfood"), "receipt.dogfood")?;
    let scope = str_field(receipt.get("scope"), "receipt.scope")?;
    let passed = gates
        .iter()
        .filter(|g| g.get("status").and_then(Value::as_str) == Some("pass"))
        .count();
    let dogfood_summary = dogfood.get("summary").filter(|v| v.is_object());
    let verdict = dogfood_summary
        .and_then(|v| v.get("verdict"))
        .filter(|v| v.is_object());
    let rows = render_gate_rows(gates)?;
    let raw_json = serde_json::to_string_pretty(receipt)?;
    Ok(format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>ChaosControl replay readiness</title>
<style>
:root {{ color-scheme: light dark; --ok:#138a36; --bad:#b42318; --warn:#b7791f; --muted:#667085; --border:#98a2b3; }}
body {{ font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; margin: 2rem; line-height: 1.45; }}
header {{ border-bottom: 1px solid var(--border); margin-bottom: 1.5rem; padding-bottom: 1rem; }}
.grid {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(14rem, 1fr)); gap: 1rem; }}
.card {{ border: 1px solid var(--border); border-radius: 0.75rem; padding: 1rem; }}
.card h2 {{ font-size: 0.95rem; margin: 0 0 0.4rem; color: var(--muted); }}
.value {{ font-size: 1.35rem; font-weight: 700; }}
.pill {{ border-radius: 999px; color: white; display: inline-block; font-weight: 700; padding: 0.15rem 0.55rem; }}
.ok {{ background: var(--ok); }} .bad {{ background: var(--bad); }} .warn {{ background: var(--warn); }}
table {{ border-collapse: collapse; margin-top: 1rem; width: 100%; }}
th, td {{ border-bottom: 1px solid var(--border); padding: 0.55rem; text-align: left; vertical-align: top; }}
code, pre {{ background: rgba(127,127,127,.14); border-radius: .35rem; padding: .1rem .25rem; }}
pre {{ overflow-x: auto; padding: 1rem; }}
.scope {{ border-left: .35rem solid var(--warn); padding-left: .8rem; }}
</style>
</head>
<body>
<header>
<h1>ChaosControl replay readiness</h1>
<p><code>{}</code></p>
</header>
<section class="grid" aria-label="Replay readiness summary">
<div class="card"><h2>Status</h2><div class="value"><span class="pill {}">{}</span></div></div>
<div class="card"><h2>Exit code</h2><div class="value">{}</div></div>
<div class="card"><h2>Static gates</h2><div class="value">{}/{}</div></div>
<div class="card"><h2>Failed phase</h2><div class="value">{}</div></div>
</section>
<section>
<h2>Dogfood proof rail</h2>
<div class="grid">
<div class="card"><h2>Workload</h2><div class="value">{}</div></div>
<div class="card"><h2>Dogfood status</h2><div class="value"><span class="pill {}">{}</span></div></div>
<div class="card"><h2>Expectation</h2><div class="value"><span class="pill {}">{}</span></div></div>
<div class="card"><h2>Evidence curation</h2><div class="value">{}</div></div>
<div class="card"><h2>Accepted</h2><div class="value">{}</div></div>
<div class="card"><h2>Replay class</h2><div class="value">{}</div></div>
<div class="card"><h2>Replay-parent depth</h2><div class="value">{}</div></div>
<div class="card"><h2>Seed / fail-after</h2><div class="value">{} / {}</div></div>
</div>
<p>Dogfood output: <code>{}</code></p>
</section>
<section>
<h2>Static gates</h2>
<table><thead><tr><th>Gate</th><th>Status</th><th>Command</th></tr></thead><tbody>
{}
</tbody></table>
</section>
<section>
<h2>Scope</h2>
<p class="scope">{}</p>
<p>Started: <time>{}</time>; finished: <time>{}</time>.</p>
</section>
<section>
<h2>Raw receipt</h2>
<pre>{}</pre>
</section>
</body>
</html>
"#,
        esc(summary_line),
        token_class(status),
        esc(status),
        esc_value(receipt.get("exit_code")),
        passed,
        gates.len(),
        esc_value(receipt.get("failed_phase")),
        esc_value(dogfood.get("selected_workload")),
        token_class(str_field(dogfood.get("status"), "receipt.dogfood.status")?),
        esc(str_field(dogfood.get("status"), "receipt.dogfood.status")?),
        token_class(&json_display(
            dogfood
                .get("expectation_status")
                .unwrap_or(&Value::String("not-applicable".into()))
        )),
        esc_value(dogfood.get("expectation_status")),
        esc_value(dogfood.get("evidence_curation")),
        esc_value(dogfood_summary.and_then(|v| v.get("accepted"))),
        esc_value(verdict.and_then(|v| v.get("replay_class"))),
        esc_value(verdict.and_then(|v| v.get("replay_parent_depth"))),
        esc_value(dogfood_summary.and_then(|v| v.get("seed"))),
        esc_value(dogfood_summary.and_then(|v| v.get("snapshot_probe_fail_after"))),
        esc_value(dogfood.get("output")),
        rows,
        esc(scope),
        esc_value(receipt.get("started_at")),
        esc_value(receipt.get("finished_at")),
        esc(&raw_json)
    ))
}

pub fn update_readme_status_path(
    receipt_path: impl AsRef<Path>,
    readme_path: impl AsRef<Path>,
) -> EvidenceResult<String> {
    let summary = summarize_receipt_path(receipt_path)?;
    let replacement = render_readme_status_block(&summary);
    let readme_path = readme_path.as_ref();
    let existing = std::fs::read_to_string(readme_path)?;
    let updated = replace_readme_marker_block(&existing, &replacement)?;
    if updated != existing {
        std::fs::write(readme_path, updated)?;
    }
    Ok(summary)
}

pub fn render_readme_status_block(summary_line: &str) -> String {
    format!("{README_START_MARKER}\n> **Replay readiness:** `{summary_line}`\n>\n> This is a bounded committed-evidence signal for ChaosControl's Antithesis-alternative rail: static contracts, accepted proof manifests, and optional selected dogfood evidence. It is not a claim of universal determinism or hosted-product parity.\n{README_END_MARKER}")
}

pub fn replace_readme_marker_block(readme_text: &str, replacement: &str) -> EvidenceResult<String> {
    let start = readme_text
        .find(README_START_MARKER)
        .ok_or_else(|| EvidenceError::new("README status markers missing or out of order"))?;
    let end = readme_text
        .find(README_END_MARKER)
        .ok_or_else(|| EvidenceError::new("README status markers missing or out of order"))?;
    ensure(
        end >= start,
        "README status markers missing or out of order",
    )?;
    let end = end + README_END_MARKER.len();
    Ok(format!(
        "{}{}{}",
        &readme_text[..start],
        replacement,
        &readme_text[end..]
    ))
}

pub fn check_readiness_surface_drift(
    root: impl AsRef<Path>,
    flake_path: impl AsRef<Path>,
) -> EvidenceResult<Vec<String>> {
    let root = root.as_ref();
    let flake_text = std::fs::read_to_string(flake_path.as_ref())?;
    let gate_names = validate_gate_metadata(&flake_text)?;
    let summary_line = validate_renderer_equivalence(root)?;
    Ok(vec![
        format!("static_gates={}", gate_names.join(",")),
        format!("summary={summary_line}"),
    ])
}

pub fn run_readiness_surface_drift_selftest(root: impl AsRef<Path>) -> EvidenceResult<()> {
    let root = root.as_ref();
    let flake_text = std::fs::read_to_string(root.join("flake.nix"))?;
    validate_gate_metadata(&flake_text)?;
    validate_renderer_equivalence(root)?;
    let missing = flake_text.replace("                  (\"readiness-promotion\", \"check-readiness-promotion-gate --root .\", os.environ[\"READINESS_PROMOTION_STATUS\"]),\n", "");
    match validate_gate_metadata(&missing) {
        Err(err) if err.message().contains("missing from receipt metadata") => {}
        Err(err) => {
            return Err(EvidenceError::new(format!(
                "unexpected missing-gate error: {}",
                err.message()
            )))
        }
        Ok(_) => {
            return Err(EvidenceError::new(
                "missing receipt gate fixture unexpectedly passed",
            ))
        }
    }
    let extra = flake_text.replace("              ]\n              receipt = {", "                  (\"phantom-gate\", \"python scripts/phantom.py\", os.environ[\"CONTRACT_REGISTRY_STATUS\"]),\n              ]\n              receipt = {");
    match validate_gate_metadata(&extra) {
        Err(err) if err.message().contains("without executed run_gate") => Ok(()),
        Err(err) => Err(EvidenceError::new(format!(
            "unexpected extra-gate error: {}",
            err.message()
        ))),
        Ok(_) => Err(EvidenceError::new(
            "extra receipt gate fixture unexpectedly passed",
        )),
    }
}

pub fn validate_gate_metadata(flake_text: &str) -> EvidenceResult<Vec<String>> {
    let executed = executed_static_gate_names(flake_text)?;
    let receipt = receipt_static_gate_names(flake_text)?;
    let executed_set = executed.iter().collect::<BTreeSet<_>>();
    let receipt_set = receipt.iter().collect::<BTreeSet<_>>();
    let missing = executed
        .iter()
        .filter(|name| !receipt_set.contains(name))
        .cloned()
        .collect::<Vec<_>>();
    let extra = receipt
        .iter()
        .filter(|name| !executed_set.contains(name))
        .cloned()
        .collect::<Vec<_>>();
    ensure(
        missing.is_empty(),
        format!(
            "executed static gates missing from receipt metadata: {}",
            missing.join(", ")
        ),
    )?;
    ensure(
        extra.is_empty(),
        format!(
            "receipt static gates without executed run_gate: {}",
            extra.join(", ")
        ),
    )?;
    Ok(executed)
}

pub fn sample_replay_readiness_receipt(dogfood: bool, status: &str) -> Value {
    let dogfood_obj = if dogfood {
        json!({"selected_workload":"rust-workload","status":"pass","output":"/tmp/proof&artifact","summary":{"accepted":true,"seed":42,"snapshot_probe_fail_after":25,"verdict":{"replay_class":"snapshot_backed_reproduced","replay_parent_depth":2}},"expectation":{"expected":{"accepted":true}},"expectation_status":"matched","evidence_curation":"explicit-follow-up"})
    } else {
        json!({"selected_workload":null,"status":"skipped","output":null,"summary":null,"expectation":null,"expectation_status":"not-applicable","evidence_curation":"explicit-follow-up"})
    };
    json!({"schema_version":1,"command":"replay-readiness","status":status,"exit_code": if status == "passed" {0} else {1},"failed_phase": if status == "passed" {Value::Null} else {Value::String("evidence-contracts".into())},"started_at":"2026-05-08T00:00:00Z","finished_at":"2026-05-08T00:00:01Z","static_gates":[{"name":"contract-registry","command":"check-contract-registry .","status":"pass"},{"name":"evidence-contracts","command":"check-evidence-contracts --root .","status": if status == "passed" {"pass"} else {"fail"}}],"dogfood":dogfood_obj,"scope":"bounded committed replay/evidence readiness; not universal determinism or hosted-product parity"})
}

pub fn sample_decision_receipt() -> Value {
    json!({
        "schema_version": 1,
        "command": "replay-readiness-decision-receipt",
        "status": "recorded",
        "generated_at": "2026-05-11T00:00:00Z",
        "scope": "bounded local operator review receipt; not a shared decision store, hosted service, scheduler, or product-parity claim",
        "raw_log_scraping": false,
        "source": {
            "fleet_index": "target/fleet-triage-index.html",
            "receipt_paths": ["target/replay-readiness-receipt.json"]
        },
        "decisions": [
            {
                "decision_id": "local-review-0001",
                "receipt_path": "target/replay-readiness-receipt.json",
                "operator": "local-operator",
                "action": "reproduce",
                "rationale": "Replay class is snapshot_backed_reproduced; run reproduce/minimize before promotion.",
                "replay_class": "snapshot_backed_reproduced",
                "linked_artifacts": [
                    "target/fleet-triage-index.html",
                    "dogfood-results/raft-accepted-verdict-dogfood-20260509T030143Z/verdict.json"
                ],
                "recorded_at": "2026-05-11T00:00:00Z"
            }
        ],
        "anti_claims": [
            "This is not a shared decision store.",
            "This is not a hosted service or fleet scheduler.",
            "This decision receipt requires no raw-log scraping and does not prove product parity."
        ]
    })
}

pub fn sample_scheduler_receipt() -> Value {
    json!({
        "schema_version": 1,
        "command": "replay-readiness-scheduler-receipt",
        "status": "planned",
        "generated_at": "2026-05-11T00:00:00Z",
        "scope": "bounded local replay run manifest; not a hosted service, not a fleet-scale scheduler, not a shared queue, and not product-parity evidence",
        "raw_log_scraping": false,
        "source_decision_receipt": "target/decision-receipt.json",
        "schedule": {
            "mode": "manual-batch",
            "max_runs": 2,
            "concurrency": 1
        },
        "run_plan": [
            {
                "run_id": "local-run-raft-0001",
                "workload": "raft",
                "command": "replay-readiness --dogfood raft --receipt target/raft-replay-readiness.json",
                "receipt_path": "target/raft-replay-readiness.json",
                "decision_policy": "record-local-decision"
            },
            {
                "run_id": "local-run-redb-0001",
                "workload": "redb",
                "command": "replay-readiness --dogfood redb --receipt target/redb-replay-readiness.json",
                "receipt_path": "target/redb-replay-readiness.json",
                "decision_policy": "record-local-decision"
            }
        ],
        "anti_claims": [
            "This is not a hosted service.",
            "This is not a fleet-scale scheduler and not a shared queue.",
            "This scheduler receipt uses no raw-log scraping and does not prove product parity."
        ]
    })
}

fn validate_renderer_equivalence(_root: &Path) -> EvidenceResult<String> {
    let receipt = sample_replay_readiness_receipt(true, "passed");
    let summary = summarize_receipt(&receipt)?;
    ensure(
        summary.starts_with("replay-readiness status="),
        "summary line has unexpected prefix",
    )?;
    ensure(
        summary.contains("scope=bounded"),
        "summary line lost bounded scope token",
    )?;
    let dashboard = render_dashboard(&receipt, &summary)?;
    ensure(
        dashboard.contains(&summary),
        "dashboard does not contain summary line",
    )?;
    ensure(
        dashboard.contains("snapshot_backed_reproduced"),
        "dashboard lost dogfood replay class",
    )?;
    let fleet_index = render_fleet_triage_index(&[(
        "receipt.json".to_string(),
        receipt.clone(),
        summary.clone(),
    )])?;
    ensure(
        fleet_index.contains("ChaosControl fleet triage index")
            && fleet_index.contains("snapshot_backed_reproduced"),
        "fleet triage index lost receipt summary or replay class",
    )?;
    ensure(
        bounded(&fleet_index) && fleet_index.contains("not a hosted service"),
        "fleet triage index lost hosted-service anti-overclaim language",
    )?;
    let decision = sample_decision_receipt();
    let decision_summary = validate_decision_receipt(&decision)?;
    ensure(
        decision_summary.contains("scope=bounded-local-not-shared"),
        "decision receipt summary lost bounded local scope",
    )?;
    let mut overclaimed_decision = decision.clone();
    overclaimed_decision["scope"] = json!("hosted shared decision store");
    match validate_decision_receipt(&overclaimed_decision) {
        Err(_) => {}
        Ok(_) => {
            return Err(EvidenceError::new(
                "overclaiming decision receipt unexpectedly passed",
            ))
        }
    }
    let scheduler = sample_scheduler_receipt();
    let scheduler_summary = validate_scheduler_receipt(&scheduler)?;
    ensure(
        scheduler_summary.contains("scope=bounded-local-not-hosted")
            && scheduler_summary.contains("runs=2"),
        "scheduler receipt summary lost bounded local scope or run count",
    )?;
    let mut overclaimed_scheduler = scheduler.clone();
    overclaimed_scheduler["scope"] = json!("hosted fleet-scale scheduler");
    match validate_scheduler_receipt(&overclaimed_scheduler) {
        Err(_) => {}
        Ok(_) => {
            return Err(EvidenceError::new(
                "overclaiming scheduler receipt unexpectedly passed",
            ))
        }
    }
    let execution = json!({
        "schema_version": 1,
        "command": "replay-readiness-scheduler-execution",
        "status": "passed",
        "plan_path": "scheduler.json",
        "started_at_unix": 1,
        "finished_at_unix": 2,
        "scope": "bounded local sequential scheduler execution receipt; not a hosted service, not a fleet-scale scheduler, not a shared queue, and not product-parity evidence",
        "raw_log_scraping": false,
        "schedule": {"mode": "manual-batch", "max_runs": 1, "concurrency": 1},
        "runs": [{
            "run_id": "local-run-raft-0001",
            "workload": "raft",
            "command": "replay-readiness --receipt target/raft.json",
            "receipt_path": "target/raft.json",
            "decision_policy": "record-local-decision",
            "started_at_unix": 1,
            "finished_at_unix": 2,
            "exit_code": 0,
            "status": "passed",
            "receipt_summary": "replay-readiness status=passed"
        }],
        "anti_claims": [
            "This is not a hosted service.",
            "This is not a fleet-scale scheduler and not a shared queue.",
            "This scheduler execution receipt captures command status and receipt summaries without raw-log scraping."
        ]
    });
    ensure(
        validate_scheduler_execution_receipt(&execution)?
            .contains("scope=bounded-local-sequential-not-hosted"),
        "scheduler execution summary lost bounded local scope",
    )?;
    let mut overclaimed_execution = execution;
    overclaimed_execution["raw_log_scraping"] = json!(true);
    match validate_scheduler_execution_receipt(&overclaimed_execution) {
        Err(_) => {}
        Ok(_) => {
            return Err(EvidenceError::new(
                "raw-log scheduler execution unexpectedly passed",
            ))
        }
    }
    let fleet_scheduler = sample_fleet_scheduler_receipt();
    let fleet_scheduler_summary = validate_fleet_scheduler_receipt(&fleet_scheduler)?;
    ensure(
        fleet_scheduler_summary.contains("scope=bounded-hosted-fleet"),
        "fleet scheduler summary lost bounded hosted/fleet scope",
    )?;
    let mut overclaimed_fleet_scheduler = fleet_scheduler;
    overclaimed_fleet_scheduler["raw_log_scraping"] = json!(true);
    match validate_fleet_scheduler_receipt(&overclaimed_fleet_scheduler) {
        Err(_) => {}
        Ok(_) => {
            return Err(EvidenceError::new(
                "raw-log fleet scheduler receipt unexpectedly passed",
            ))
        }
    }
    match render_fleet_triage_index(&[]) {
        Err(_) => {}
        Ok(_) => return Err(EvidenceError::new("empty fleet index unexpectedly passed")),
    }
    ensure(
        bounded(&dashboard),
        "dashboard lost bounded anti-overclaim language",
    )?;
    let readme =
        format!("# Demo\n\n{README_START_MARKER}\nold status\n{README_END_MARKER}\n\nafter\n");
    let rendered = replace_readme_marker_block(&readme, &render_readme_status_block(&summary))?;
    ensure(
        rendered.contains(&summary),
        "README snippet does not contain summary line",
    )?;
    ensure(
        bounded(&rendered),
        "README snippet lost bounded anti-overclaim language",
    )?;
    match replace_readme_marker_block("# Demo\n", &render_readme_status_block(&summary)) {
        Err(_) => Ok(summary),
        Ok(_) => Err(EvidenceError::new(
            "README updater accepted missing status markers",
        )),
    }
}

fn executed_static_gate_names(flake_text: &str) -> EvidenceResult<Vec<String>> {
    let block = between(
        flake_text,
        "echo \"== replay readiness: static checks ==\"",
        "echo \"replay readiness checks passed\"",
    )?;
    let mut names = Vec::new();
    for line in block.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("run_gate ") {
            if let Some(name) = rest.split_whitespace().next() {
                names.push(name.to_string());
            }
        }
    }
    validate_unique_nonempty(names, "replay-readiness run_gate entries")
}

fn receipt_static_gate_names(flake_text: &str) -> EvidenceResult<Vec<String>> {
    let block = between(
        flake_text,
        "              gates = [",
        "              ]\n              receipt = {",
    )?;
    let mut names = Vec::new();
    for line in block.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("(\"") {
            if let Some((name, _)) = rest.split_once("\",") {
                names.push(name.to_string());
            }
        }
    }
    validate_unique_nonempty(names, "receipt static gate metadata entries")
}

fn validate_unique_nonempty(names: Vec<String>, label: &str) -> EvidenceResult<Vec<String>> {
    ensure(!names.is_empty(), format!("no {label} found"))?;
    let set = names.iter().collect::<BTreeSet<_>>();
    ensure(
        set.len() == names.len(),
        format!("duplicate {label}: {names:?}"),
    )?;
    Ok(names)
}

fn between<'a>(text: &'a str, start_marker: &str, end_marker: &str) -> EvidenceResult<&'a str> {
    let start = text
        .find(start_marker)
        .ok_or_else(|| EvidenceError::new(format!("missing start marker: {start_marker}")))?;
    let end = text[start..]
        .find(end_marker)
        .ok_or_else(|| EvidenceError::new(format!("missing end marker: {end_marker}")))?
        + start;
    Ok(&text[start..end])
}

fn render_gate_rows(gates: &[Value]) -> EvidenceResult<String> {
    let mut rows = Vec::new();
    for gate in gates {
        let name = str_field(gate.get("name"), "gate.name")?;
        let command = str_field(gate.get("command"), "gate.command")?;
        let status = str_field(gate.get("status"), "gate.status")?;
        rows.push(format!("<tr><td>{}</td><td><span class=\"pill {}\">{}</span></td><td><code>{}</code></td></tr>", esc(name), token_class(status), esc(status), esc(command)));
    }
    Ok(rows.join("\n"))
}

fn load_json(path: &Path) -> EvidenceResult<Value> {
    let text = std::fs::read_to_string(path)
        .map_err(|err| EvidenceError::new(format!("{}: {err}", path.display())))?;
    serde_json::from_str(&text).map_err(Into::into)
}

fn str_field<'a>(value: Option<&'a Value>, field: &str) -> EvidenceResult<&'a str> {
    match value.and_then(Value::as_str) {
        Some(text) if !text.is_empty() => Ok(text),
        _ => Err(EvidenceError::new(format!(
            "{field}: expected non-empty string"
        ))),
    }
}

fn token_field<'a>(value: Option<&'a Value>, field: &str) -> EvidenceResult<&'a str> {
    let text = str_field(value, field)?;
    ensure(
        !text.chars().any(char::is_whitespace),
        format!("{field}: expected whitespace-free string"),
    )?;
    Ok(text)
}

fn optional_token<'a>(value: Option<&'a Value>, field: &str) -> EvidenceResult<Option<&'a str>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        other => token_field(other, field).map(Some),
    }
}

fn object_field<'a>(
    value: Option<&'a Value>,
    field: &str,
) -> EvidenceResult<&'a serde_json::Map<String, Value>> {
    value
        .and_then(Value::as_object)
        .ok_or_else(|| EvidenceError::new(format!("{field}: expected object")))
}

fn array_field<'a>(value: Option<&'a Value>, field: &str) -> EvidenceResult<&'a Vec<Value>> {
    value
        .and_then(Value::as_array)
        .ok_or_else(|| EvidenceError::new(format!("{field}: expected non-empty list")))
}

fn int_field(value: Option<&Value>, field: &str) -> EvidenceResult<i64> {
    value
        .and_then(Value::as_i64)
        .ok_or_else(|| EvidenceError::new(format!("{field}: expected integer")))
}

fn optional_int(value: Option<&Value>, field: &str) -> EvidenceResult<Option<i64>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        other => int_field(other, field).map(Some),
    }
}

fn bool_field(value: Option<&Value>, field: &str) -> EvidenceResult<bool> {
    value
        .and_then(Value::as_bool)
        .ok_or_else(|| EvidenceError::new(format!("{field}: expected boolean")))
}

fn json_display(value: &Value) -> String {
    match value {
        Value::Null => "none".to_string(),
        Value::Bool(true) => "true".to_string(),
        Value::Bool(false) => "false".to_string(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn esc_value(value: Option<&Value>) -> String {
    esc(&value
        .map(json_display)
        .unwrap_or_else(|| "none".to_string()))
}

fn esc(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn token_class(value: &str) -> &'static str {
    if matches!(value, "passed" | "pass" | "matched" | "skipped") {
        "ok"
    } else if matches!(value, "failed" | "fail") || value.starts_with("mismatched") {
        "bad"
    } else {
        "warn"
    }
}

fn bounded(text: &str) -> bool {
    let lowered = text.to_lowercase();
    lowered.contains("bounded")
        && (lowered.contains("not universal") || lowered.contains("not a claim of universal"))
}
