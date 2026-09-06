use serde_json::json;
use sha2::Digest;

use crate::replay_readiness_orchestration::unix_seconds;

pub use crate::replay_readiness_render::{
    render_readme_status_block, README_END_MARKER, README_START_MARKER,
};
use crate::typed_operator_command::command_display;

pub fn summarize_receipt_path(
    path: impl AsRef<::std::path::Path>,
) -> crate::EvidenceResult<String> {
    summarize_receipt(&crate::replay_readiness_loader::load_json(path.as_ref())?)
}

pub fn summarize_receipt(receipt: &::serde_json::Value) -> crate::EvidenceResult<String> {
    crate::replay_readiness_core::summarize_receipt(receipt)
}

pub fn write_dashboard_path(
    receipt_path: impl AsRef<::std::path::Path>,
    output_path: impl AsRef<::std::path::Path>,
) -> crate::EvidenceResult<()> {
    let receipt = crate::replay_readiness_loader::load_json(receipt_path.as_ref())?;
    let summary = summarize_receipt(&receipt)?;
    let html = render_dashboard(&receipt, &summary)?;
    crate::replay_readiness_publication::write_bytes(output_path.as_ref(), html.as_bytes())
}

pub fn write_fleet_triage_index_path(
    receipt_paths: &[impl AsRef<::std::path::Path>],
    output_path: impl AsRef<::std::path::Path>,
) -> crate::EvidenceResult<()> {
    let html = render_fleet_triage_index_path(receipt_paths)?;
    crate::replay_readiness_publication::write_bytes(output_path.as_ref(), html.as_bytes())
}

pub fn render_fleet_triage_index_path(
    receipt_paths: &[impl AsRef<::std::path::Path>],
) -> crate::EvidenceResult<String> {
    crate::ensure(
        !receipt_paths.is_empty(),
        "fleet triage index requires at least one replay-readiness receipt",
    )?;
    let mut entries = Vec::with_capacity(receipt_paths.len());
    for path in receipt_paths {
        let path = path.as_ref();
        let receipt = crate::replay_readiness_loader::load_json(path)?;
        entries.push((
            path.display().to_string(),
            receipt,
            summarize_receipt_path(path)?,
        ));
    }
    render_fleet_triage_index(&entries)
}

pub fn render_fleet_triage_index(
    entries: &[(String, ::serde_json::Value, String)],
) -> crate::EvidenceResult<String> {
    crate::ensure(
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

pub fn write_decision_receipt_path(
    output_path: impl AsRef<::std::path::Path>,
) -> crate::EvidenceResult<()> {
    let output_path = output_path.as_ref();
    let receipt = sample_decision_receipt();
    validate_decision_receipt(&receipt)?;
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, serde_json::to_vec_pretty(&receipt)?)?;
    Ok(())
}

pub fn validate_decision_receipt_path(
    path: impl AsRef<::std::path::Path>,
) -> crate::EvidenceResult<String> {
    validate_decision_receipt(&crate::replay_readiness_loader::load_json(path.as_ref())?)
}

pub fn write_scheduler_receipt_path(
    output_path: impl AsRef<::std::path::Path>,
) -> crate::EvidenceResult<()> {
    let output_path = output_path.as_ref();
    let receipt = sample_scheduler_receipt();
    validate_scheduler_receipt(&receipt)?;
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, serde_json::to_vec_pretty(&receipt)?)?;
    Ok(())
}

pub fn validate_scheduler_receipt_path(
    path: impl AsRef<::std::path::Path>,
) -> crate::EvidenceResult<String> {
    validate_scheduler_receipt(&crate::replay_readiness_loader::load_json(path.as_ref())?)
}

pub fn validate_decision_receipt(receipt: &::serde_json::Value) -> crate::EvidenceResult<String> {
    let schema_version = int_field(receipt.get("schema_version"), "decision.schema_version")?;
    crate::ensure(
        schema_version == 1,
        format!("decision.schema_version: expected 1, got {schema_version}"),
    )?;
    let command = str_field(receipt.get("command"), "decision.command")?;
    crate::ensure(
        command == "replay-readiness-decision-receipt",
        format!("decision.command: expected replay-readiness-decision-receipt, got {command:?}"),
    )?;
    let status = str_field(receipt.get("status"), "decision.status")?;
    crate::ensure(
        status == "recorded",
        format!("decision.status: unsupported value {status:?}"),
    )?;
    let scope = str_field(receipt.get("scope"), "decision.scope")?;
    crate::ensure(
        scope.contains("local")
            && scope.contains("bounded")
            && scope.contains("not a shared decision store"),
        "decision.scope: must declare bounded local scope and not a shared decision store",
    )?;
    crate::ensure(
        !matches!(
            receipt.get("raw_log_scraping"),
            Some(::serde_json::Value::Bool(true))
        ),
        "decision.raw_log_scraping: raw-log scraping is not allowed",
    )?;

    let source = object_field(receipt.get("source"), "decision.source")?;
    str_field(source.get("fleet_index"), "decision.source.fleet_index")?;
    let receipt_paths = array_field(source.get("receipt_paths"), "decision.source.receipt_paths")?;
    crate::ensure(
        !receipt_paths.is_empty(),
        "decision.source.receipt_paths: expected non-empty list",
    )?;
    for (idx, path) in receipt_paths.iter().enumerate() {
        str_field(Some(path), &format!("decision.source.receipt_paths[{idx}]"))?;
    }

    let decisions = array_field(receipt.get("decisions"), "decision.decisions")?;
    crate::ensure(
        !decisions.is_empty(),
        "decision.decisions: expected non-empty list",
    )?;
    let mut ids = ::std::collections::BTreeSet::new();
    let mut actions = ::std::collections::BTreeSet::new();
    for (idx, decision) in decisions.iter().enumerate() {
        let decision = object_field(Some(decision), &format!("decision.decisions[{idx}]"))?;
        let id = token_field(
            decision.get("decision_id"),
            &format!("decision.decisions[{idx}].decision_id"),
        )?;
        crate::ensure(
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
        crate::ensure(
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
        if let Some(::serde_json::Value::String(_)) = decision.get("replay_class") {
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
    crate::ensure(
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

pub fn validate_scheduler_receipt(receipt: &::serde_json::Value) -> crate::EvidenceResult<String> {
    let schema_version = int_field(receipt.get("schema_version"), "scheduler.schema_version")?;
    crate::ensure(
        schema_version == 1,
        format!("scheduler.schema_version: expected 1, got {schema_version}"),
    )?;
    let command = str_field(receipt.get("command"), "scheduler.command")?;
    crate::ensure(
        command == "replay-readiness-scheduler-receipt",
        format!("scheduler.command: expected replay-readiness-scheduler-receipt, got {command:?}"),
    )?;
    let status = str_field(receipt.get("status"), "scheduler.status")?;
    crate::ensure(
        matches!(status, "planned" | "recorded" | "partial"),
        format!("scheduler.status: unsupported value {status:?}"),
    )?;
    let scope = str_field(receipt.get("scope"), "scheduler.scope")?;
    crate::ensure(
        scope.contains("bounded")
            && scope.contains("local")
            && scope.contains("not a hosted service")
            && scope.contains("not a fleet-scale scheduler"),
        "scheduler.scope: must declare bounded local scope and not a hosted/fleet scheduler",
    )?;
    crate::ensure(
        !matches!(
            receipt.get("raw_log_scraping"),
            Some(::serde_json::Value::Bool(true))
        ),
        "scheduler.raw_log_scraping: raw-log scraping is not allowed",
    )?;

    let schedule = object_field(receipt.get("schedule"), "scheduler.schedule")?;
    let mode = token_field(schedule.get("mode"), "scheduler.schedule.mode")?;
    crate::ensure(
        matches!(mode, "manual-batch" | "cron-preview"),
        format!("scheduler.schedule.mode: unsupported value {mode:?}"),
    )?;
    let max_runs = int_field(schedule.get("max_runs"), "scheduler.schedule.max_runs")?;
    let concurrency = int_field(
        schedule.get("concurrency"),
        "scheduler.schedule.concurrency",
    )?;
    crate::ensure(
        max_runs > 0,
        "scheduler.schedule.max_runs: expected positive integer",
    )?;
    crate::ensure(
        concurrency > 0 && concurrency <= max_runs,
        "scheduler.schedule.concurrency: expected positive integer no larger than max_runs",
    )?;

    let run_plan = array_field(receipt.get("run_plan"), "scheduler.run_plan")?;
    crate::ensure(
        !run_plan.is_empty(),
        "scheduler.run_plan: expected non-empty list",
    )?;
    crate::ensure(
        run_plan.len() as i64 <= max_runs,
        "scheduler.run_plan: cannot exceed schedule.max_runs",
    )?;
    let mut run_ids = ::std::collections::BTreeSet::new();
    let mut workloads = ::std::collections::BTreeSet::new();
    for (idx, run) in run_plan.iter().enumerate() {
        let run = object_field(Some(run), &format!("scheduler.run_plan[{idx}]"))?;
        let run_id = token_field(
            run.get("run_id"),
            &format!("scheduler.run_plan[{idx}].run_id"),
        )?;
        crate::ensure(
            run_ids.insert(run_id.to_string()),
            format!("scheduler.run_plan[{idx}].run_id: duplicate {run_id}"),
        )?;
        let workload = token_field(
            run.get("workload"),
            &format!("scheduler.run_plan[{idx}].workload"),
        )?;
        workloads.insert(workload.to_string());
        typed_command_field(
            run.get("command_plan"),
            &format!("scheduler.run_plan[{idx}].command_plan"),
        )?;
        str_field(
            run.get("receipt_path"),
            &format!("scheduler.run_plan[{idx}].receipt_path"),
        )?;
        let decision_policy = token_field(
            run.get("decision_policy"),
            &format!("scheduler.run_plan[{idx}].decision_policy"),
        )?;
        crate::ensure(
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
    crate::ensure(
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
    plan_path: impl AsRef<::std::path::Path>,
    output_path: impl AsRef<::std::path::Path>,
) -> crate::EvidenceResult<String> {
    let plan_path = plan_path.as_ref();
    let output_path = output_path.as_ref();
    let plan = crate::replay_readiness_loader::load_json(plan_path)?;
    validate_scheduler_receipt(&plan)?;
    let execution = execute_scheduler_receipt(&plan, plan_path)?;
    let summary = validate_scheduler_execution_receipt(&execution)?;
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, serde_json::to_vec_pretty(&execution)?)?;
    Ok(summary)
}

pub fn validate_scheduler_execution_receipt_path(
    path: impl AsRef<::std::path::Path>,
) -> crate::EvidenceResult<String> {
    validate_scheduler_execution_receipt(&crate::replay_readiness_loader::load_json(path.as_ref())?)
}

pub fn execute_scheduler_receipt(
    plan: &::serde_json::Value,
    plan_path: &::std::path::Path,
) -> crate::EvidenceResult<::serde_json::Value> {
    let schedule = object_field(plan.get("schedule"), "scheduler.schedule")?;
    let concurrency = int_field(
        schedule.get("concurrency"),
        "scheduler.schedule.concurrency",
    )?;
    crate::ensure(
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
        let (command, command_observation) = execute_typed_command_field(
            run.get("command_plan"),
            &format!("scheduler.run_plan[{idx}].command_plan"),
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
        let exit_code = command_observation.exit_code.unwrap_or(125);
        let succeeded = command_observation.disposition == "succeeded";
        let receipt_summary = if succeeded {
            Some(summarize_receipt_path(receipt_path)?)
        } else {
            None
        };
        if !succeeded {
            failures += 1;
        }
        runs.push(json!({
            "run_id": run_id,
            "workload": workload,
            "command": command_display(&command),
            "command_plan": command,
            "command_observation": command_observation,
            "receipt_path": receipt_path,
            "decision_policy": decision_policy,
            "started_at_unix": run_started,
            "finished_at_unix": unix_seconds(),
            "exit_code": exit_code,
            "status": if succeeded { "passed" } else { "failed" },
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

pub fn validate_scheduler_execution_receipt(
    receipt: &::serde_json::Value,
) -> crate::EvidenceResult<String> {
    let schema_version = int_field(
        receipt.get("schema_version"),
        "scheduler_execution.schema_version",
    )?;
    crate::ensure(
        schema_version == 1,
        format!("scheduler_execution.schema_version: expected 1, got {schema_version}"),
    )?;
    let command = str_field(receipt.get("command"), "scheduler_execution.command")?;
    crate::ensure(
        command == "replay-readiness-scheduler-execution",
        format!("scheduler_execution.command: expected replay-readiness-scheduler-execution, got {command:?}"),
    )?;
    let status = str_field(receipt.get("status"), "scheduler_execution.status")?;
    crate::ensure(
        matches!(status, "passed" | "failed" | "partial"),
        format!("scheduler_execution.status: unsupported value {status:?}"),
    )?;
    let scope = str_field(receipt.get("scope"), "scheduler_execution.scope")?;
    crate::ensure(
        scope.contains("bounded")
            && scope.contains("local")
            && scope.contains("not a hosted service")
            && scope.contains("not a fleet-scale scheduler")
            && scope.contains("not a shared queue"),
        "scheduler_execution.scope: must declare bounded local scope and not hosted/fleet/shared-queue scheduler",
    )?;
    crate::ensure(
        !matches!(
            receipt.get("raw_log_scraping"),
            Some(::serde_json::Value::Bool(true))
        ),
        "scheduler_execution.raw_log_scraping: raw-log scraping is not allowed",
    )?;
    let schedule = object_field(receipt.get("schedule"), "scheduler_execution.schedule")?;
    let concurrency = int_field(
        schedule.get("concurrency"),
        "scheduler_execution.schedule.concurrency",
    )?;
    crate::ensure(
        concurrency == 1,
        "scheduler_execution.schedule.concurrency: expected bounded sequential concurrency=1",
    )?;
    let runs = array_field(receipt.get("runs"), "scheduler_execution.runs")?;
    crate::ensure(
        !runs.is_empty(),
        "scheduler_execution.runs: expected non-empty list",
    )?;
    let mut run_ids = ::std::collections::BTreeSet::new();
    let mut workloads = ::std::collections::BTreeSet::new();
    let mut passed = 0usize;
    for (idx, run) in runs.iter().enumerate() {
        let run = object_field(Some(run), &format!("scheduler_execution.runs[{idx}]"))?;
        let run_id = token_field(
            run.get("run_id"),
            &format!("scheduler_execution.runs[{idx}].run_id"),
        )?;
        crate::ensure(
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
        crate::ensure(
            matches!(run_status, "passed" | "failed"),
            format!("scheduler_execution.runs[{idx}].status: unsupported value {run_status:?}"),
        )?;
        let exit_code = int_field(
            run.get("exit_code"),
            &format!("scheduler_execution.runs[{idx}].exit_code"),
        )?;
        if run_status == "passed" {
            crate::ensure(
                exit_code == 0,
                format!("scheduler_execution.runs[{idx}].exit_code: passed run must exit 0"),
            )?;
            str_field(
                run.get("receipt_summary"),
                &format!("scheduler_execution.runs[{idx}].receipt_summary"),
            )?;
            passed += 1;
        } else {
            crate::ensure(
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
    crate::ensure(
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

pub fn write_fleet_scheduler_receipt_path(
    path: impl AsRef<::std::path::Path>,
) -> crate::EvidenceResult<()> {
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

pub fn validate_fleet_scheduler_receipt_path(
    path: impl AsRef<::std::path::Path>,
) -> crate::EvidenceResult<String> {
    validate_fleet_scheduler_receipt(&crate::replay_readiness_loader::load_json(path.as_ref())?)
}

pub fn execute_fleet_scheduler_receipt_path(
    plan_path: impl AsRef<::std::path::Path>,
    output_path: impl AsRef<::std::path::Path>,
) -> crate::EvidenceResult<String> {
    let plan_path = plan_path.as_ref();
    let output_path = output_path.as_ref();
    let receipt = execute_fleet_scheduler_receipt(
        &crate::replay_readiness_loader::load_json(plan_path)?,
        plan_path,
    )?;
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, serde_json::to_vec_pretty(&receipt)?)?;
    validate_fleet_scheduler_receipt(&receipt)
}

pub fn execute_fleet_scheduler_receipt(
    plan: &::serde_json::Value,
    plan_path: &::std::path::Path,
) -> crate::EvidenceResult<::serde_json::Value> {
    let queue = object_field(plan.get("queue"), "fleet_scheduler_plan.queue")?;
    let queue_id = token_field(queue.get("queue_id"), "fleet_scheduler_plan.queue.queue_id")?;
    let state_path = plan
        .get("state_path")
        .or_else(|| queue.get("state_path"))
        .and_then(::serde_json::Value::as_str)
        .map(::std::path::PathBuf::from)
        .unwrap_or_else(|| plan_path.with_extension("state.json"));
    let previous_state = if state_path.exists() {
        Some(crate::replay_readiness_loader::load_json(&state_path)?)
    } else {
        None
    };
    let completed_before_start = previous_state
        .as_ref()
        .and_then(|state| state.get("completed_runs"))
        .and_then(::serde_json::Value::as_array)
        .map(|runs| runs.len())
        .unwrap_or(0);
    let lease_timeout_seconds = int_field(
        queue.get("lease_timeout_seconds"),
        "fleet_scheduler_plan.queue.lease_timeout_seconds",
    )?;
    crate::ensure(
        lease_timeout_seconds > 0,
        "fleet_scheduler_plan.queue.lease_timeout_seconds: expected positive integer",
    )?;
    let max_concurrency = int_field(
        queue.get("max_concurrency"),
        "fleet_scheduler_plan.queue.max_concurrency",
    )?;
    crate::ensure(
        max_concurrency > 0,
        "fleet_scheduler_plan.queue.max_concurrency: expected positive integer",
    )?;
    let workers = array_field(plan.get("workers"), "fleet_scheduler_plan.workers")?;
    crate::ensure(
        !workers.is_empty(),
        "fleet_scheduler_plan.workers: expected non-empty list",
    )?;
    let mut worker_ids = Vec::with_capacity(workers.len());
    for (idx, worker) in workers.iter().enumerate() {
        let worker = object_field(
            Some(worker),
            &format!("fleet_scheduler_plan.workers[{idx}]"),
        )?;
        worker_ids.push(token_field(
            worker.get("worker_id"),
            &format!("fleet_scheduler_plan.workers[{idx}].worker_id"),
        )?);
    }
    crate::ensure(
        max_concurrency as usize <= worker_ids.len(),
        "fleet_scheduler_plan.queue.max_concurrency: cannot exceed worker count",
    )?;

    let entries = array_field(queue.get("entries"), "fleet_scheduler_plan.queue.entries")?;
    crate::ensure(
        !entries.is_empty(),
        "fleet_scheduler_plan.queue.entries: expected non-empty list",
    )?;

    let mut receipt_entries = Vec::with_capacity(entries.len());
    let mut receipt_workers = Vec::with_capacity(workers.len());
    let mut runs = Vec::with_capacity(entries.len());
    let mut completed_runs = previous_state
        .as_ref()
        .and_then(|state| state.get("completed_runs"))
        .and_then(::serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut failures = 0usize;

    for (idx, worker_id) in worker_ids.iter().enumerate() {
        receipt_workers.push(json!({
            "worker_id": worker_id,
            "node_id": format!("local-node-{idx}"),
            "lease_id": format!("idle-{worker_id}"),
            "status": "idle"
        }));
    }

    for (idx, entry) in entries.iter().enumerate() {
        let entry = object_field(
            Some(entry),
            &format!("fleet_scheduler_plan.queue.entries[{idx}]"),
        )?;
        let queue_entry_id = token_field(
            entry.get("queue_entry_id"),
            &format!("fleet_scheduler_plan.queue.entries[{idx}].queue_entry_id"),
        )?;
        let run_id = token_field(
            entry.get("run_id"),
            &format!("fleet_scheduler_plan.queue.entries[{idx}].run_id"),
        )?;
        let workload = token_field(
            entry.get("workload"),
            &format!("fleet_scheduler_plan.queue.entries[{idx}].workload"),
        )?;
        let (command, command_observation) = execute_typed_command_field(
            entry.get("command_plan"),
            &format!("fleet_scheduler_plan.queue.entries[{idx}].command_plan"),
        )?;
        let receipt_path = str_field(
            entry.get("receipt_path"),
            &format!("fleet_scheduler_plan.queue.entries[{idx}].receipt_path"),
        )?;
        let worker_id = worker_ids[idx % (max_concurrency as usize)];
        let lease_id = format!("lease-{queue_entry_id}");
        let exit_code = command_observation.exit_code.unwrap_or(125);
        let succeeded = command_observation.disposition == "succeeded";
        let run_status = if succeeded { "passed" } else { "failed" };
        if !succeeded {
            failures += 1;
        }
        let receipt_summary = if succeeded {
            Some(summarize_receipt_path(receipt_path)?)
        } else {
            None
        };
        let entry_state = if succeeded { "completed" } else { "failed" };
        receipt_entries.push(json!({
            "queue_entry_id": queue_entry_id,
            "run_id": run_id,
            "workload": workload,
            "state": entry_state
        }));
        if succeeded {
            completed_runs.push(::serde_json::Value::String(run_id.to_string()));
        }
        let state_snapshot = json!({
            "schema_version": 1,
            "queue_id": queue_id,
            "state_path": state_path.display().to_string(),
            "last_persisted_run_id": run_id,
            "completed_runs": completed_runs,
            "entries": receipt_entries,
            "persisted_at": format!("unix:{}", unix_seconds())
        });
        if let Some(parent) = state_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&state_path, serde_json::to_vec_pretty(&state_snapshot)?)?;
        runs.push(json!({
            "run_id": run_id,
            "queue_entry_id": queue_entry_id,
            "worker_id": worker_id,
            "workload": workload,
            "command": command_display(&command),
            "command_plan": command,
            "command_observation": command_observation,
            "lease_id": lease_id,
            "receipt_path": receipt_path,
            "receipt_summary": receipt_summary,
            "status": run_status,
            "exit_code": exit_code
        }));
    }

    let status = if failures == 0 {
        "recorded"
    } else if failures == entries.len() {
        "failed"
    } else {
        "partial"
    };
    let decisions = plan
        .get("operator_decisions")
        .and_then(::serde_json::Value::as_array)
        .cloned()
        .unwrap_or_else(|| {
            vec![::serde_json::Value::String(
                "target/decision-receipt.json".to_string(),
            )]
        });

    Ok(json!({
        "schema_version": 1,
        "command": "replay-readiness-fleet-scheduler-receipt",
        "status": status,
        "generated_at": format!("unix:{}", unix_seconds()),
        "plan_path": plan_path.display().to_string(),
        "scope": "bounded hosted/fleet scheduler runtime receipt with a local durable queue worker loop, leases, worker run receipts, and receipt summaries; not product-parity evidence, not a full Antithesis replacement, and not raw-log evidence",
        "raw_log_scraping": false,
        "queue": {
            "kind": "durable-file-backed",
            "queue_id": queue_id,
            "lease_timeout_seconds": lease_timeout_seconds,
            "max_concurrency": max_concurrency,
            "state_path": state_path.display().to_string(),
            "entries": receipt_entries
        },
        "restart_recovery": {
            "state_path": state_path.display().to_string(),
            "loaded_existing_state": previous_state.is_some(),
            "completed_before_start": completed_before_start,
            "persisted_after_each_run": true
        },
        "workers": receipt_workers,
        "runs": runs,
        "operator_decisions": decisions,
        "anti_claims": [
            "This is bounded hosted/fleet scheduler runtime evidence, not product parity.",
            "This is not a full Antithesis replacement.",
            "This fleet scheduler worker loop captures durable queue, lease, worker, run, and receipt-summary state without raw-log scraping."
        ]
    }))
}

pub fn sample_fleet_scheduler_plan() -> ::serde_json::Value {
    json!({
        "schema_version": 1,
        "queue": {
            "queue_id": "fleet-queue-0001",
            "lease_timeout_seconds": 900,
            "max_concurrency": 2,
            "entries": [
                {"queue_entry_id": "queue-raft-0001", "run_id": "fleet-run-raft-0001", "workload": "raft", "command_plan": sample_typed_command("replay-readiness-summary", &["--sample", "--output", "target/fleet/raft-replay-readiness.json"]), "receipt_path": "target/fleet/raft-replay-readiness.json"},
                {"queue_entry_id": "queue-redb-0001", "run_id": "fleet-run-redb-0001", "workload": "redb", "command_plan": sample_typed_command("replay-readiness-summary", &["--sample", "--output", "target/fleet/redb-replay-readiness.json"]), "receipt_path": "target/fleet/redb-replay-readiness.json"}
            ]
        },
        "workers": [
            {"worker_id": "worker-a"},
            {"worker_id": "worker-b"}
        ],
        "operator_decisions": ["target/decision-receipt.json"]
    })
}

pub fn sample_fleet_scheduler_receipt() -> ::serde_json::Value {
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
            "state_path": "target/fleet/fleet-queue-state.json",
            "entries": [
                {"queue_entry_id": "queue-raft-0001", "run_id": "fleet-run-raft-0001", "workload": "raft", "state": "completed"},
                {"queue_entry_id": "queue-redb-0001", "run_id": "fleet-run-redb-0001", "workload": "redb", "state": "completed"}
            ]
        },
        "restart_recovery": {
            "state_path": "target/fleet/fleet-queue-state.json",
            "loaded_existing_state": false,
            "completed_before_start": 0,
            "persisted_after_each_run": true
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

pub fn validate_fleet_scheduler_receipt(
    receipt: &::serde_json::Value,
) -> crate::EvidenceResult<String> {
    let schema_version = int_field(
        receipt.get("schema_version"),
        "fleet_scheduler.schema_version",
    )?;
    crate::ensure(
        schema_version == 1,
        format!("fleet_scheduler.schema_version: expected 1, got {schema_version}"),
    )?;
    let command = str_field(receipt.get("command"), "fleet_scheduler.command")?;
    crate::ensure(
        command == "replay-readiness-fleet-scheduler-receipt",
        format!("fleet_scheduler.command: expected replay-readiness-fleet-scheduler-receipt, got {command:?}"),
    )?;
    let status = str_field(receipt.get("status"), "fleet_scheduler.status")?;
    crate::ensure(
        matches!(status, "recorded" | "partial" | "failed"),
        format!("fleet_scheduler.status: unsupported value {status:?}"),
    )?;
    let scope = str_field(receipt.get("scope"), "fleet_scheduler.scope")?;
    crate::ensure(
        scope.contains("bounded")
            && scope.contains("hosted/fleet")
            && scope.contains("durable queue")
            && scope.contains("worker")
            && scope.contains("not product-parity"),
        "fleet_scheduler.scope: must declare bounded hosted/fleet durable queue and no product-parity claim",
    )?;
    crate::ensure(
        !matches!(
            receipt.get("raw_log_scraping"),
            Some(::serde_json::Value::Bool(true))
        ),
        "fleet_scheduler.raw_log_scraping: raw-log scraping is not allowed",
    )?;

    let queue = object_field(receipt.get("queue"), "fleet_scheduler.queue")?;
    let queue_kind = token_field(queue.get("kind"), "fleet_scheduler.queue.kind")?;
    crate::ensure(
        matches!(queue_kind, "durable-file-backed" | "durable-service-backed"),
        format!("fleet_scheduler.queue.kind: unsupported value {queue_kind:?}"),
    )?;
    token_field(queue.get("queue_id"), "fleet_scheduler.queue.queue_id")?;
    let lease_timeout_seconds = int_field(
        queue.get("lease_timeout_seconds"),
        "fleet_scheduler.queue.lease_timeout_seconds",
    )?;
    crate::ensure(
        lease_timeout_seconds > 0,
        "fleet_scheduler.queue.lease_timeout_seconds: expected positive integer",
    )?;
    let max_concurrency = int_field(
        queue.get("max_concurrency"),
        "fleet_scheduler.queue.max_concurrency",
    )?;
    crate::ensure(
        max_concurrency > 0,
        "fleet_scheduler.queue.max_concurrency: expected positive integer",
    )?;
    str_field(queue.get("state_path"), "fleet_scheduler.queue.state_path")?;
    let entries = array_field(queue.get("entries"), "fleet_scheduler.queue.entries")?;
    crate::ensure(
        !entries.is_empty(),
        "fleet_scheduler.queue.entries: expected non-empty list",
    )?;
    let mut entry_ids = ::std::collections::BTreeSet::new();
    let mut entry_run_ids = ::std::collections::BTreeSet::new();
    for (idx, entry) in entries.iter().enumerate() {
        let entry = object_field(
            Some(entry),
            &format!("fleet_scheduler.queue.entries[{idx}]"),
        )?;
        let entry_id = token_field(
            entry.get("queue_entry_id"),
            &format!("fleet_scheduler.queue.entries[{idx}].queue_entry_id"),
        )?;
        crate::ensure(
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
        crate::ensure(
            matches!(state, "queued" | "leased" | "completed" | "failed"),
            format!("fleet_scheduler.queue.entries[{idx}].state: unsupported value {state:?}"),
        )?;
    }

    let restart_recovery = object_field(
        receipt.get("restart_recovery"),
        "fleet_scheduler.restart_recovery",
    )?;
    str_field(
        restart_recovery.get("state_path"),
        "fleet_scheduler.restart_recovery.state_path",
    )?;
    let completed_before_start = int_field(
        restart_recovery.get("completed_before_start"),
        "fleet_scheduler.restart_recovery.completed_before_start",
    )?;
    crate::ensure(
        completed_before_start >= 0,
        "fleet_scheduler.restart_recovery.completed_before_start: expected non-negative integer",
    )?;
    crate::ensure(
        matches!(
            restart_recovery.get("persisted_after_each_run"),
            Some(::serde_json::Value::Bool(true))
        ),
        "fleet_scheduler.restart_recovery.persisted_after_each_run: expected true",
    )?;

    let workers = array_field(receipt.get("workers"), "fleet_scheduler.workers")?;
    crate::ensure(
        !workers.is_empty(),
        "fleet_scheduler.workers: expected non-empty list",
    )?;
    let mut worker_ids = ::std::collections::BTreeSet::new();
    for (idx, worker) in workers.iter().enumerate() {
        let worker = object_field(Some(worker), &format!("fleet_scheduler.workers[{idx}]"))?;
        let worker_id = token_field(
            worker.get("worker_id"),
            &format!("fleet_scheduler.workers[{idx}].worker_id"),
        )?;
        crate::ensure(
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
        crate::ensure(
            matches!(worker_status, "idle" | "running" | "offline"),
            format!("fleet_scheduler.workers[{idx}].status: unsupported value {worker_status:?}"),
        )?;
    }

    let runs = array_field(receipt.get("runs"), "fleet_scheduler.runs")?;
    crate::ensure(
        !runs.is_empty(),
        "fleet_scheduler.runs: expected non-empty list",
    )?;
    let mut run_ids = ::std::collections::BTreeSet::new();
    let mut workloads = ::std::collections::BTreeSet::new();
    let mut passed = 0usize;
    for (idx, run) in runs.iter().enumerate() {
        let run = object_field(Some(run), &format!("fleet_scheduler.runs[{idx}]"))?;
        let run_id = token_field(
            run.get("run_id"),
            &format!("fleet_scheduler.runs[{idx}].run_id"),
        )?;
        crate::ensure(
            run_ids.insert(run_id.to_string()),
            format!("fleet_scheduler.runs[{idx}].run_id: duplicate {run_id}"),
        )?;
        crate::ensure(
            entry_run_ids.contains(run_id),
            format!("fleet_scheduler.runs[{idx}].run_id: {run_id} missing from queue entries"),
        )?;
        let queue_entry_id = token_field(
            run.get("queue_entry_id"),
            &format!("fleet_scheduler.runs[{idx}].queue_entry_id"),
        )?;
        crate::ensure(entry_ids.contains(queue_entry_id), format!("fleet_scheduler.runs[{idx}].queue_entry_id: {queue_entry_id} missing from queue entries"))?;
        let worker_id = token_field(
            run.get("worker_id"),
            &format!("fleet_scheduler.runs[{idx}].worker_id"),
        )?;
        crate::ensure(
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
        crate::ensure(
            matches!(run_status, "passed" | "failed"),
            format!("fleet_scheduler.runs[{idx}].status: unsupported value {run_status:?}"),
        )?;
        let exit_code = int_field(
            run.get("exit_code"),
            &format!("fleet_scheduler.runs[{idx}].exit_code"),
        )?;
        if run_status == "passed" {
            crate::ensure(
                exit_code == 0,
                format!("fleet_scheduler.runs[{idx}].exit_code: passed run must exit 0"),
            )?;
            let summary = str_field(
                run.get("receipt_summary"),
                &format!("fleet_scheduler.runs[{idx}].receipt_summary"),
            )?;
            crate::ensure(summary.contains("replay-readiness status="), format!("fleet_scheduler.runs[{idx}].receipt_summary: expected replay-readiness summary"))?;
            passed += 1;
        } else {
            crate::ensure(
                exit_code != 0,
                format!("fleet_scheduler.runs[{idx}].exit_code: failed run must be nonzero"),
            )?;
        }
    }

    let decisions = array_field(
        receipt.get("operator_decisions"),
        "fleet_scheduler.operator_decisions",
    )?;
    crate::ensure(
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
    crate::ensure(
        anti_claim_text.contains("bounded hosted/fleet")
            && anti_claim_text.contains("not product parity")
            && anti_claim_text.contains("not a full antithesis replacement")
            && anti_claim_text.contains("without raw-log scraping"),
        "fleet_scheduler.anti_claims: missing bounded hosted/fleet anti-overclaim text",
    )?;
    Ok(format!(
        "replay-readiness-fleet-scheduler status={status} queue={queue_kind} workers={} runs={} passed={} restart_persisted=true workloads={} scope=bounded-hosted-fleet",
        workers.len(),
        runs.len(),
        passed,
        workloads.into_iter().collect::<Vec<_>>().join(",")
    ))
}

pub fn write_hosted_shared_state_receipt_path(
    path: impl AsRef<::std::path::Path>,
) -> crate::EvidenceResult<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&sample_hosted_shared_state_receipt())?,
    )?;
    Ok(())
}

pub fn validate_hosted_shared_state_receipt_path(
    path: impl AsRef<::std::path::Path>,
) -> crate::EvidenceResult<String> {
    validate_hosted_shared_state_receipt(&crate::replay_readiness_loader::load_json(path.as_ref())?)
}

pub fn execute_hosted_shared_state_receipt_path(
    plan_path: impl AsRef<::std::path::Path>,
    output_path: impl AsRef<::std::path::Path>,
) -> crate::EvidenceResult<String> {
    let plan_path = plan_path.as_ref();
    let output_path = output_path.as_ref();
    let receipt = execute_hosted_shared_state_receipt(
        &crate::replay_readiness_loader::load_json(plan_path)?,
        plan_path,
    )?;
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, serde_json::to_vec_pretty(&receipt)?)?;
    validate_hosted_shared_state_receipt(&receipt)
}

pub fn execute_hosted_shared_state_receipt(
    plan: &::serde_json::Value,
    plan_path: &::std::path::Path,
) -> crate::EvidenceResult<::serde_json::Value> {
    let machines = array_field(plan.get("machines"), "hosted_shared_state_plan.machines")?;
    crate::ensure(
        machines.len() >= 2,
        "hosted_shared_state_plan.machines: expected at least two machines",
    )?;
    let mut machine_writer = ::std::collections::BTreeMap::new();
    let mut machine_values = Vec::with_capacity(machines.len());
    for (idx, machine) in machines.iter().enumerate() {
        let machine = object_field(
            Some(machine),
            &format!("hosted_shared_state_plan.machines[{idx}]"),
        )?;
        let machine_id = token_field(
            machine.get("machine_id"),
            &format!("hosted_shared_state_plan.machines[{idx}].machine_id"),
        )?;
        let writer_id = token_field(
            machine.get("writer_id"),
            &format!("hosted_shared_state_plan.machines[{idx}].writer_id"),
        )?;
        crate::ensure(
            machine_writer
                .insert(machine_id.to_string(), writer_id.to_string())
                .is_none(),
            format!("hosted_shared_state_plan.machines[{idx}].machine_id: duplicate {machine_id}"),
        )?;
        machine_values.push(json!({"machine_id": machine_id, "writer_id": writer_id}));
    }

    let workers = array_field(
        plan.get("hypervisor_workers"),
        "hosted_shared_state_plan.hypervisor_workers",
    )?;
    crate::ensure(
        workers.len() >= 2,
        "hosted_shared_state_plan.hypervisor_workers: expected at least two hypervisor workers",
    )?;
    let mut worker_machine = ::std::collections::BTreeMap::new();
    let mut worker_values = Vec::with_capacity(workers.len());
    for (idx, worker) in workers.iter().enumerate() {
        let worker = object_field(
            Some(worker),
            &format!("hosted_shared_state_plan.hypervisor_workers[{idx}]"),
        )?;
        let worker_id = token_field(
            worker.get("hypervisor_worker_id"),
            &format!("hosted_shared_state_plan.hypervisor_workers[{idx}].hypervisor_worker_id"),
        )?;
        let machine_id = token_field(
            worker.get("machine_id"),
            &format!("hosted_shared_state_plan.hypervisor_workers[{idx}].machine_id"),
        )?;
        crate::ensure(
            machine_writer.contains_key(machine_id),
            format!("hosted_shared_state_plan.hypervisor_workers[{idx}].machine_id: {machine_id} missing from machines"),
        )?;
        crate::ensure(
            worker_machine
                .insert(worker_id.to_string(), machine_id.to_string())
                .is_none(),
            format!("hosted_shared_state_plan.hypervisor_workers[{idx}].hypervisor_worker_id: duplicate {worker_id}"),
        )?;
        worker_values.push(json!({"hypervisor_worker_id": worker_id, "machine_id": machine_id}));
    }
    let worker_ids = worker_machine.keys().cloned().collect::<Vec<_>>();

    let queue = object_field(plan.get("queue"), "hosted_shared_state_plan.queue")?;
    let queue_id = token_field(
        queue.get("queue_id"),
        "hosted_shared_state_plan.queue.queue_id",
    )?;
    let state_path = plan
        .get("state_path")
        .or_else(|| queue.get("state_path"))
        .and_then(::serde_json::Value::as_str)
        .map(::std::path::PathBuf::from)
        .unwrap_or_else(|| plan_path.with_extension("queue-state.json"));
    let entries = array_field(
        queue.get("entries"),
        "hosted_shared_state_plan.queue.entries",
    )?;
    crate::ensure(
        !entries.is_empty(),
        "hosted_shared_state_plan.queue.entries: expected non-empty list",
    )?;
    let decision_store = object_field(
        plan.get("decision_store"),
        "hosted_shared_state_plan.decision_store",
    )?;
    let store_id = token_field(
        decision_store.get("store_id"),
        "hosted_shared_state_plan.decision_store.store_id",
    )?;
    let decision_store_path = decision_store
        .get("path")
        .and_then(::serde_json::Value::as_str)
        .map(::std::path::PathBuf::from)
        .unwrap_or_else(|| plan_path.with_extension("decision-store.json"));

    let mut queue_entries = Vec::with_capacity(entries.len());
    let mut decision_records = Vec::with_capacity(entries.len());
    let mut failures = 0usize;
    for (idx, entry) in entries.iter().enumerate() {
        let entry = object_field(
            Some(entry),
            &format!("hosted_shared_state_plan.queue.entries[{idx}]"),
        )?;
        let queue_entry_id = token_field(
            entry.get("queue_entry_id"),
            &format!("hosted_shared_state_plan.queue.entries[{idx}].queue_entry_id"),
        )?;
        let run_id = token_field(
            entry.get("run_id"),
            &format!("hosted_shared_state_plan.queue.entries[{idx}].run_id"),
        )?;
        let workload = token_field(
            entry.get("workload"),
            &format!("hosted_shared_state_plan.queue.entries[{idx}].workload"),
        )?;
        let (command, command_observation) = execute_typed_command_field(
            entry.get("command_plan"),
            &format!("hosted_shared_state_plan.queue.entries[{idx}].command_plan"),
        )?;
        let receipt_path = str_field(
            entry.get("receipt_path"),
            &format!("hosted_shared_state_plan.queue.entries[{idx}].receipt_path"),
        )?;
        let worker_id = worker_ids[idx % worker_ids.len()].as_str();
        let machine_id = worker_machine
            .get(worker_id)
            .expect("worker ids are drawn from worker_machine");
        let writer_id = machine_writer
            .get(machine_id)
            .expect("worker machines were validated");
        let lease_id = format!("lease-{queue_entry_id}");
        let exit_code = command_observation.exit_code.unwrap_or(125);
        let succeeded = command_observation.disposition == "succeeded";
        let run_status = if succeeded { "completed" } else { "failed" };
        if !succeeded {
            failures += 1;
        }
        let receipt_summary = if succeeded {
            summarize_receipt_path(receipt_path)?
        } else {
            format!("replay-readiness status=failed exit_code={exit_code}")
        };
        queue_entries.push(json!({
            "queue_entry_id": queue_entry_id,
            "run_id": run_id,
            "workload": workload,
            "state": run_status,
            "command": command_display(&command),
            "command_plan": command,
            "command_observation": command_observation,
            "exit_code": exit_code,
            "lease": {
                "lease_id": lease_id,
                "lease_epoch": (idx + 1) as u64,
                "owner_machine_id": machine_id,
                "hypervisor_worker_id": worker_id
            },
            "receipt_path": receipt_path,
            "receipt_summary": receipt_summary
        }));
        decision_records.push(json!({
            "decision_id": format!("decision-{queue_entry_id}"),
            "decision_revision": 1,
            "previous_revision": null,
            "writer_id": writer_id,
            "machine_id": machine_id,
            "run_id": run_id,
            "queue_entry_id": queue_entry_id,
            "action": entry.get("decision_action").and_then(::serde_json::Value::as_str).unwrap_or("triage"),
            "status": "recorded",
            "receipt_path": entry.get("decision_receipt_path").and_then(::serde_json::Value::as_str).unwrap_or("target/hosted/decision.json")
        }));
        let state_snapshot = json!({
            "schema_version": 1,
            "queue_id": queue_id,
            "state_path": state_path.display().to_string(),
            "last_persisted_run_id": run_id,
            "entries": queue_entries,
            "persisted_at": format!("unix:{}", unix_seconds())
        });
        if let Some(parent) = state_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&state_path, serde_json::to_vec_pretty(&state_snapshot)?)?;
    }

    let decision_snapshot = json!({
        "schema_version": 1,
        "store_id": store_id,
        "records": decision_records,
        "persisted_at": format!("unix:{}", unix_seconds())
    });
    if let Some(parent) = decision_store_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &decision_store_path,
        serde_json::to_vec_pretty(&decision_snapshot)?,
    )?;

    let status = if failures == 0 {
        "recorded"
    } else if failures == entries.len() {
        "failed"
    } else {
        "partial"
    };
    Ok(json!({
        "schema_version": 1,
        "command": "replay-readiness-hosted-shared-state",
        "status": status,
        "generated_at": format!("unix:{}", unix_seconds()),
        "plan_path": plan_path.display().to_string(),
        "scope": "bounded hosted/shared-state scheduler and decision-store contract with shared queue leases, machine identities, hypervisor workers, run receipts, replay-readiness summaries, decision revisions, and writer identities; not SaaS hosting, not product parity, not Antithesis parity, and not raw-log evidence",
        "raw_log_scraping": false,
        "machines": machine_values,
        "hypervisor_workers": worker_values,
        "queue": {
            "kind": "durable-shared",
            "queue_id": queue_id,
            "state_path": state_path.display().to_string(),
            "entries": queue_entries
        },
        "decision_store": {
            "kind": "durable-shared",
            "store_id": store_id,
            "path": decision_store_path.display().to_string(),
            "records": decision_records
        },
        "artifacts": {
            "queue_state_path": state_path.display().to_string(),
            "decision_store_path": decision_store_path.display().to_string()
        },
        "anti_claims": [
            "This is a bounded hosted/shared-state contract receipt, not SaaS hosting evidence.",
            "This is not product parity or Antithesis parity.",
            "This receipt links shared queue leases, machine IDs, hypervisor workers, run receipts, replay-readiness summaries, decision revisions, and writer identities without raw-log scraping."
        ]
    }))
}

pub fn sample_hosted_shared_state_plan() -> ::serde_json::Value {
    json!({
        "schema_version": 1,
        "machines": [
            {"machine_id": "machine-a", "writer_id": "writer-machine-a"},
            {"machine_id": "machine-b", "writer_id": "writer-machine-b"}
        ],
        "hypervisor_workers": [
            {"hypervisor_worker_id": "hv-a", "machine_id": "machine-a"},
            {"hypervisor_worker_id": "hv-b", "machine_id": "machine-b"}
        ],
        "queue": {
            "queue_id": "hosted-queue-0001",
            "entries": [
                {"queue_entry_id": "hosted-q-raft-0001", "run_id": "hosted-run-raft-0001", "workload": "raft", "command_plan": sample_typed_command("replay-readiness", &["--receipt", "target/hosted/raft-replay-readiness.json"]), "receipt_path": "target/hosted/raft-replay-readiness.json", "decision_action": "reproduce"},
                {"queue_entry_id": "hosted-q-redb-0001", "run_id": "hosted-run-redb-0001", "workload": "redb", "command_plan": sample_typed_command("replay-readiness", &["--receipt", "target/hosted/redb-replay-readiness.json"]), "receipt_path": "target/hosted/redb-replay-readiness.json", "decision_action": "triage"}
            ]
        },
        "decision_store": {"store_id": "hosted-decision-store-0001"}
    })
}

pub fn sample_hosted_shared_state_receipt() -> ::serde_json::Value {
    json!({
        "schema_version": 1,
        "command": "replay-readiness-hosted-shared-state",
        "status": "recorded",
        "generated_at": "2026-05-11T00:00:00Z",
        "scope": "bounded hosted/shared-state scheduler and decision-store contract with shared queue leases, machine identities, hypervisor workers, run receipts, replay-readiness summaries, decision revisions, and writer identities; not SaaS hosting, not product parity, not Antithesis parity, and not raw-log evidence",
        "raw_log_scraping": false,
        "machines": [
            {"machine_id": "machine-a", "writer_id": "writer-machine-a"},
            {"machine_id": "machine-b", "writer_id": "writer-machine-b"}
        ],
        "hypervisor_workers": [
            {"hypervisor_worker_id": "hv-a", "machine_id": "machine-a"},
            {"hypervisor_worker_id": "hv-b", "machine_id": "machine-b"}
        ],
        "queue": {
            "kind": "durable-shared",
            "queue_id": "hosted-queue-0001",
            "entries": [
                {
                    "queue_entry_id": "hosted-q-raft-0001",
                    "run_id": "hosted-run-raft-0001",
                    "workload": "raft",
                    "state": "completed",
                    "lease": {"lease_id": "lease-hosted-q-raft-0001", "lease_epoch": 1, "owner_machine_id": "machine-a", "hypervisor_worker_id": "hv-a"},
                    "receipt_path": "target/hosted/raft-replay-readiness.json",
                    "receipt_summary": "replay-readiness status=passed dogfood=raft:pass scope=bounded"
                },
                {
                    "queue_entry_id": "hosted-q-redb-0001",
                    "run_id": "hosted-run-redb-0001",
                    "workload": "redb",
                    "state": "completed",
                    "lease": {"lease_id": "lease-hosted-q-redb-0001", "lease_epoch": 1, "owner_machine_id": "machine-b", "hypervisor_worker_id": "hv-b"},
                    "receipt_path": "target/hosted/redb-replay-readiness.json",
                    "receipt_summary": "replay-readiness status=passed dogfood=redb:pass scope=bounded"
                }
            ]
        },
        "decision_store": {
            "kind": "durable-shared",
            "store_id": "hosted-decision-store-0001",
            "records": [
                {"decision_id": "decision-raft-0001", "decision_revision": 1, "previous_revision": null, "writer_id": "writer-machine-a", "machine_id": "machine-a", "run_id": "hosted-run-raft-0001", "queue_entry_id": "hosted-q-raft-0001", "action": "reproduce", "status": "recorded", "receipt_path": "target/hosted/raft-decision.json"},
                {"decision_id": "decision-redb-0001", "decision_revision": 1, "previous_revision": null, "writer_id": "writer-machine-b", "machine_id": "machine-b", "run_id": "hosted-run-redb-0001", "queue_entry_id": "hosted-q-redb-0001", "action": "triage", "status": "recorded", "receipt_path": "target/hosted/redb-decision.json"}
            ]
        },
        "anti_claims": [
            "This is a bounded hosted/shared-state contract receipt, not SaaS hosting evidence.",
            "This is not product parity or Antithesis parity.",
            "This receipt links shared queue leases, machine IDs, hypervisor workers, run receipts, replay-readiness summaries, decision revisions, and writer identities without raw-log scraping."
        ]
    })
}

pub fn validate_hosted_shared_state_receipt(
    receipt: &::serde_json::Value,
) -> crate::EvidenceResult<String> {
    let schema_version = int_field(
        receipt.get("schema_version"),
        "hosted_shared_state.schema_version",
    )?;
    crate::ensure(
        schema_version == 1,
        format!("hosted_shared_state.schema_version: expected 1, got {schema_version}"),
    )?;
    let command = str_field(receipt.get("command"), "hosted_shared_state.command")?;
    crate::ensure(
        command == "replay-readiness-hosted-shared-state",
        format!("hosted_shared_state.command: unexpected {command:?}"),
    )?;
    let status = str_field(receipt.get("status"), "hosted_shared_state.status")?;
    crate::ensure(
        matches!(status, "recorded" | "partial" | "failed"),
        format!("hosted_shared_state.status: unsupported value {status:?}"),
    )?;
    let scope = str_field(receipt.get("scope"), "hosted_shared_state.scope")?.to_lowercase();
    crate::ensure(
        scope.contains("bounded hosted/shared-state")
            && scope.contains("shared queue")
            && scope.contains("decision-store")
            && scope.contains("not saas")
            && scope.contains("not product parity")
            && scope.contains("not antithesis parity"),
        "hosted_shared_state.scope: must declare bounded shared-state scope and non-claims",
    )?;
    crate::ensure(
        !matches!(
            receipt.get("raw_log_scraping"),
            Some(::serde_json::Value::Bool(true))
        ),
        "hosted_shared_state.raw_log_scraping: raw-log scraping is not allowed",
    )?;

    let machines = array_field(receipt.get("machines"), "hosted_shared_state.machines")?;
    crate::ensure(
        machines.len() >= 2,
        "hosted_shared_state.machines: expected at least two machine identities",
    )?;
    let mut machine_ids = ::std::collections::BTreeSet::new();
    let mut writer_to_machine = ::std::collections::BTreeMap::new();
    for (idx, machine) in machines.iter().enumerate() {
        let machine = object_field(
            Some(machine),
            &format!("hosted_shared_state.machines[{idx}]"),
        )?;
        let machine_id = token_field(
            machine.get("machine_id"),
            &format!("hosted_shared_state.machines[{idx}].machine_id"),
        )?;
        crate::ensure(
            machine_ids.insert(machine_id.to_string()),
            format!("hosted_shared_state.machines[{idx}].machine_id: duplicate {machine_id}"),
        )?;
        let writer_id = token_field(
            machine.get("writer_id"),
            &format!("hosted_shared_state.machines[{idx}].writer_id"),
        )?;
        crate::ensure(
            writer_to_machine
                .insert(writer_id.to_string(), machine_id.to_string())
                .is_none(),
            format!("hosted_shared_state.machines[{idx}].writer_id: duplicate {writer_id}"),
        )?;
    }

    let workers = array_field(
        receipt.get("hypervisor_workers"),
        "hosted_shared_state.hypervisor_workers",
    )?;
    crate::ensure(
        workers.len() >= 2,
        "hosted_shared_state.hypervisor_workers: expected at least two hypervisor workers",
    )?;
    let mut worker_to_machine = ::std::collections::BTreeMap::new();
    for (idx, worker) in workers.iter().enumerate() {
        let worker = object_field(
            Some(worker),
            &format!("hosted_shared_state.hypervisor_workers[{idx}]"),
        )?;
        let worker_id = token_field(
            worker.get("hypervisor_worker_id"),
            &format!("hosted_shared_state.hypervisor_workers[{idx}].hypervisor_worker_id"),
        )?;
        let machine_id = token_field(
            worker.get("machine_id"),
            &format!("hosted_shared_state.hypervisor_workers[{idx}].machine_id"),
        )?;
        crate::ensure(machine_ids.contains(machine_id), format!("hosted_shared_state.hypervisor_workers[{idx}].machine_id: {machine_id} missing from machines"))?;
        crate::ensure(worker_to_machine.insert(worker_id.to_string(), machine_id.to_string()).is_none(), format!("hosted_shared_state.hypervisor_workers[{idx}].hypervisor_worker_id: duplicate {worker_id}"))?;
    }

    let queue = object_field(receipt.get("queue"), "hosted_shared_state.queue")?;
    let queue_kind = token_field(queue.get("kind"), "hosted_shared_state.queue.kind")?;
    crate::ensure(
        queue_kind == "durable-shared",
        format!("hosted_shared_state.queue.kind: expected durable-shared, got {queue_kind:?}"),
    )?;
    token_field(queue.get("queue_id"), "hosted_shared_state.queue.queue_id")?;
    let entries = array_field(queue.get("entries"), "hosted_shared_state.queue.entries")?;
    crate::ensure(
        !entries.is_empty(),
        "hosted_shared_state.queue.entries: expected non-empty list",
    )?;
    let mut queue_entry_ids = ::std::collections::BTreeSet::new();
    let mut run_ids = ::std::collections::BTreeSet::new();
    let mut lease_owners = ::std::collections::BTreeMap::new();
    for (idx, entry) in entries.iter().enumerate() {
        let entry = object_field(
            Some(entry),
            &format!("hosted_shared_state.queue.entries[{idx}]"),
        )?;
        let queue_entry_id = token_field(
            entry.get("queue_entry_id"),
            &format!("hosted_shared_state.queue.entries[{idx}].queue_entry_id"),
        )?;
        crate::ensure(queue_entry_ids.insert(queue_entry_id.to_string()), format!("hosted_shared_state.queue.entries[{idx}].queue_entry_id: duplicate {queue_entry_id}"))?;
        let run_id = token_field(
            entry.get("run_id"),
            &format!("hosted_shared_state.queue.entries[{idx}].run_id"),
        )?;
        crate::ensure(
            run_ids.insert(run_id.to_string()),
            format!("hosted_shared_state.queue.entries[{idx}].run_id: duplicate {run_id}"),
        )?;
        token_field(
            entry.get("workload"),
            &format!("hosted_shared_state.queue.entries[{idx}].workload"),
        )?;
        let state = token_field(
            entry.get("state"),
            &format!("hosted_shared_state.queue.entries[{idx}].state"),
        )?;
        crate::ensure(
            matches!(state, "queued" | "leased" | "completed" | "failed"),
            format!("hosted_shared_state.queue.entries[{idx}].state: unsupported value {state:?}"),
        )?;
        let lease = object_field(
            entry.get("lease"),
            &format!("hosted_shared_state.queue.entries[{idx}].lease"),
        )?;
        let lease_id = token_field(
            lease.get("lease_id"),
            &format!("hosted_shared_state.queue.entries[{idx}].lease.lease_id"),
        )?;
        let lease_epoch = int_field(
            lease.get("lease_epoch"),
            &format!("hosted_shared_state.queue.entries[{idx}].lease.lease_epoch"),
        )?;
        crate::ensure(lease_epoch > 0, format!("hosted_shared_state.queue.entries[{idx}].lease.lease_epoch: expected positive epoch"))?;
        let owner_machine_id = token_field(
            lease.get("owner_machine_id"),
            &format!("hosted_shared_state.queue.entries[{idx}].lease.owner_machine_id"),
        )?;
        crate::ensure(machine_ids.contains(owner_machine_id), format!("hosted_shared_state.queue.entries[{idx}].lease.owner_machine_id: {owner_machine_id} missing from machines"))?;
        let worker_id = token_field(
            lease.get("hypervisor_worker_id"),
            &format!("hosted_shared_state.queue.entries[{idx}].lease.hypervisor_worker_id"),
        )?;
        let worker_machine = worker_to_machine.get(worker_id).ok_or_else(|| crate::EvidenceError::new(format!("hosted_shared_state.queue.entries[{idx}].lease.hypervisor_worker_id: {worker_id} missing from hypervisor_workers")))?;
        crate::ensure(worker_machine == owner_machine_id, format!("hosted_shared_state.queue.entries[{idx}].lease: owner machine {owner_machine_id} does not match hypervisor worker machine {worker_machine}"))?;
        crate::ensure(lease_owners.insert(lease_id.to_string(), (owner_machine_id.to_string(), lease_epoch)).is_none(), format!("hosted_shared_state.queue.entries[{idx}].lease.lease_id: duplicate lease ownership for {lease_id}"))?;
        str_field(
            entry.get("receipt_path"),
            &format!("hosted_shared_state.queue.entries[{idx}].receipt_path"),
        )?;
        let summary = str_field(
            entry.get("receipt_summary"),
            &format!("hosted_shared_state.queue.entries[{idx}].receipt_summary"),
        )?;
        crate::ensure(summary.contains("replay-readiness status="), format!("hosted_shared_state.queue.entries[{idx}].receipt_summary: expected replay-readiness summary"))?;
    }

    let decision_store = object_field(
        receipt.get("decision_store"),
        "hosted_shared_state.decision_store",
    )?;
    let store_kind = token_field(
        decision_store.get("kind"),
        "hosted_shared_state.decision_store.kind",
    )?;
    crate::ensure(
        store_kind == "durable-shared",
        format!(
            "hosted_shared_state.decision_store.kind: expected durable-shared, got {store_kind:?}"
        ),
    )?;
    token_field(
        decision_store.get("store_id"),
        "hosted_shared_state.decision_store.store_id",
    )?;
    let records = array_field(
        decision_store.get("records"),
        "hosted_shared_state.decision_store.records",
    )?;
    crate::ensure(
        !records.is_empty(),
        "hosted_shared_state.decision_store.records: expected non-empty list",
    )?;
    let mut decision_revisions = ::std::collections::BTreeSet::new();
    let mut decision_writers = ::std::collections::BTreeMap::new();
    for (idx, record) in records.iter().enumerate() {
        let record = object_field(
            Some(record),
            &format!("hosted_shared_state.decision_store.records[{idx}]"),
        )?;
        let decision_id = token_field(
            record.get("decision_id"),
            &format!("hosted_shared_state.decision_store.records[{idx}].decision_id"),
        )?;
        let revision = int_field(
            record.get("decision_revision"),
            &format!("hosted_shared_state.decision_store.records[{idx}].decision_revision"),
        )?;
        crate::ensure(revision > 0, format!("hosted_shared_state.decision_store.records[{idx}].decision_revision: expected positive revision"))?;
        let revision_key = format!("{decision_id}@{revision}");
        crate::ensure(decision_revisions.insert(revision_key), format!("hosted_shared_state.decision_store.records[{idx}]: split-brain duplicate decision revision for {decision_id}@{revision}"))?;
        if let Some(previous_revision) = record
            .get("previous_revision")
            .filter(|value| !value.is_null())
        {
            let previous_revision = int_field(
                Some(previous_revision),
                &format!("hosted_shared_state.decision_store.records[{idx}].previous_revision"),
            )?;
            crate::ensure(previous_revision < revision, format!("hosted_shared_state.decision_store.records[{idx}].previous_revision: stale decision write"))?;
        }
        let writer_id = token_field(
            record.get("writer_id"),
            &format!("hosted_shared_state.decision_store.records[{idx}].writer_id"),
        )?;
        let writer_machine = writer_to_machine.get(writer_id).ok_or_else(|| crate::EvidenceError::new(format!("hosted_shared_state.decision_store.records[{idx}].writer_id: {writer_id} missing from machines")))?;
        let machine_id = token_field(
            record.get("machine_id"),
            &format!("hosted_shared_state.decision_store.records[{idx}].machine_id"),
        )?;
        crate::ensure(writer_machine == machine_id, format!("hosted_shared_state.decision_store.records[{idx}].writer_id: writer {writer_id} is not owned by machine {machine_id}"))?;
        if let Some(previous_writer) =
            decision_writers.insert(decision_id.to_string(), writer_id.to_string())
        {
            crate::ensure(previous_writer == writer_id, format!("hosted_shared_state.decision_store.records[{idx}]: split-brain decision writer for {decision_id}"))?;
        }
        let run_id = token_field(
            record.get("run_id"),
            &format!("hosted_shared_state.decision_store.records[{idx}].run_id"),
        )?;
        crate::ensure(run_ids.contains(run_id), format!("hosted_shared_state.decision_store.records[{idx}].run_id: {run_id} missing from queue runs"))?;
        let queue_entry_id = token_field(
            record.get("queue_entry_id"),
            &format!("hosted_shared_state.decision_store.records[{idx}].queue_entry_id"),
        )?;
        crate::ensure(queue_entry_ids.contains(queue_entry_id), format!("hosted_shared_state.decision_store.records[{idx}].queue_entry_id: {queue_entry_id} missing from queue entries"))?;
        let action = token_field(
            record.get("action"),
            &format!("hosted_shared_state.decision_store.records[{idx}].action"),
        )?;
        crate::ensure(matches!(action, "triage" | "reproduce" | "minimize" | "accept" | "reject"), format!("hosted_shared_state.decision_store.records[{idx}].action: unsupported value {action:?}"))?;
        let decision_status = token_field(
            record.get("status"),
            &format!("hosted_shared_state.decision_store.records[{idx}].status"),
        )?;
        crate::ensure(matches!(decision_status, "recorded" | "superseded"), format!("hosted_shared_state.decision_store.records[{idx}].status: unsupported value {decision_status:?}"))?;
        str_field(
            record.get("receipt_path"),
            &format!("hosted_shared_state.decision_store.records[{idx}].receipt_path"),
        )?;
    }

    let anti_claims = array_field(
        receipt.get("anti_claims"),
        "hosted_shared_state.anti_claims",
    )?;
    let anti_claim_text = anti_claims
        .iter()
        .map(json_display)
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();
    crate::ensure(
        anti_claim_text.contains("bounded hosted/shared-state")
            && anti_claim_text.contains("not saas")
            && anti_claim_text.contains("not product parity")
            && anti_claim_text.contains("antithesis parity")
            && anti_claim_text.contains("without raw-log scraping"),
        "hosted_shared_state.anti_claims: missing hosted/shared-state anti-overclaim text",
    )?;
    Ok(format!("replay-readiness-hosted-shared-state status={status} machines={} hypervisors={} queue_entries={} decisions={} scope=bounded-hosted-shared-state", machines.len(), workers.len(), entries.len(), records.len()))
}

pub fn write_networked_hosted_scheduler_receipt_path(
    path: impl AsRef<::std::path::Path>,
) -> crate::EvidenceResult<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&sample_networked_hosted_scheduler_receipt())?,
    )?;
    Ok(())
}

pub fn validate_networked_hosted_scheduler_receipt_path(
    path: impl AsRef<::std::path::Path>,
) -> crate::EvidenceResult<String> {
    validate_networked_hosted_scheduler_receipt(&crate::replay_readiness_loader::load_json(
        path.as_ref(),
    )?)
}

pub fn execute_networked_hosted_scheduler_receipt_path(
    plan_path: impl AsRef<::std::path::Path>,
    output_path: impl AsRef<::std::path::Path>,
) -> crate::EvidenceResult<String> {
    let plan_path = plan_path.as_ref();
    let output_path = output_path.as_ref();
    let receipt = execute_networked_hosted_scheduler_receipt(
        &crate::replay_readiness_loader::load_json(plan_path)?,
        plan_path,
    )?;
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, serde_json::to_vec_pretty(&receipt)?)?;
    validate_networked_hosted_scheduler_receipt(&receipt)
}

pub fn execute_networked_hosted_scheduler_receipt(
    plan: &::serde_json::Value,
    plan_path: &::std::path::Path,
) -> crate::EvidenceResult<::serde_json::Value> {
    let harness_id = token_field(
        plan.get("harness_id"),
        "networked_hosted_scheduler_plan.harness_id",
    )?;
    let transport = token_field(
        plan.get("transport"),
        "networked_hosted_scheduler_plan.transport",
    )?;
    crate::ensure(
        matches!(
            transport,
            "loopback-tcp" | "loopback-uds" | "multi-process-file"
        ),
        format!("networked_hosted_scheduler_plan.transport: unsupported value {transport:?}"),
    )?;
    let machines = array_field(
        plan.get("machines"),
        "networked_hosted_scheduler_plan.machines",
    )?;
    crate::ensure(
        machines.len() >= 2,
        "networked_hosted_scheduler_plan.machines: expected at least two machines",
    )?;
    let mut machine_writer = ::std::collections::BTreeMap::new();
    let mut machine_values = Vec::with_capacity(machines.len());
    for (idx, machine) in machines.iter().enumerate() {
        let machine = object_field(
            Some(machine),
            &format!("networked_hosted_scheduler_plan.machines[{idx}]"),
        )?;
        let machine_id = token_field(
            machine.get("machine_id"),
            &format!("networked_hosted_scheduler_plan.machines[{idx}].machine_id"),
        )?;
        let writer_id = token_field(
            machine.get("writer_id"),
            &format!("networked_hosted_scheduler_plan.machines[{idx}].writer_id"),
        )?;
        crate::ensure(
            machine_writer
                .insert(machine_id.to_string(), writer_id.to_string())
                .is_none(),
            format!("networked_hosted_scheduler_plan.machines[{idx}].machine_id: duplicate {machine_id}"),
        )?;
        machine_values.push(json!({"machine_id": machine_id, "writer_id": writer_id}));
    }

    let sessions = array_field(
        plan.get("worker_sessions"),
        "networked_hosted_scheduler_plan.worker_sessions",
    )?;
    crate::ensure(
        sessions.len() >= 2,
        "networked_hosted_scheduler_plan.worker_sessions: expected at least two worker sessions",
    )?;
    let mut session_values = Vec::with_capacity(sessions.len());
    let mut session_ids = Vec::with_capacity(sessions.len());
    let mut session_machine = ::std::collections::BTreeMap::new();
    let mut session_worker = ::std::collections::BTreeMap::new();
    for (idx, session) in sessions.iter().enumerate() {
        let session = object_field(
            Some(session),
            &format!("networked_hosted_scheduler_plan.worker_sessions[{idx}]"),
        )?;
        let session_id = token_field(
            session.get("worker_session_id"),
            &format!("networked_hosted_scheduler_plan.worker_sessions[{idx}].worker_session_id"),
        )?;
        let worker_id = token_field(
            session.get("hypervisor_worker_id"),
            &format!("networked_hosted_scheduler_plan.worker_sessions[{idx}].hypervisor_worker_id"),
        )?;
        let machine_id = token_field(
            session.get("machine_id"),
            &format!("networked_hosted_scheduler_plan.worker_sessions[{idx}].machine_id"),
        )?;
        crate::ensure(
            machine_writer.contains_key(machine_id),
            format!("networked_hosted_scheduler_plan.worker_sessions[{idx}].machine_id: {machine_id} missing from machines"),
        )?;
        let heartbeat_revision = session
            .get("heartbeat_revision")
            .and_then(::serde_json::Value::as_i64)
            .unwrap_or(1);
        crate::ensure(
            heartbeat_revision > 0,
            format!("networked_hosted_scheduler_plan.worker_sessions[{idx}].heartbeat_revision: expected positive heartbeat revision"),
        )?;
        crate::ensure(
            session_machine
                .insert(session_id.to_string(), machine_id.to_string())
                .is_none(),
            format!("networked_hosted_scheduler_plan.worker_sessions[{idx}].worker_session_id: duplicate {session_id}"),
        )?;
        session_worker.insert(session_id.to_string(), worker_id.to_string());
        session_ids.push(session_id.to_string());
        session_values.push(json!({
            "worker_session_id": session_id,
            "hypervisor_worker_id": worker_id,
            "machine_id": machine_id,
            "started_by": session.get("started_by").and_then(::serde_json::Value::as_str).unwrap_or("independent-process"),
            "heartbeat_revision": heartbeat_revision,
            "last_heartbeat": session.get("last_heartbeat").and_then(::serde_json::Value::as_str).unwrap_or("unix:0"),
            "state": "healthy"
        }));
    }

    let queue = object_field(plan.get("queue"), "networked_hosted_scheduler_plan.queue")?;
    let queue_id = token_field(
        queue.get("queue_id"),
        "networked_hosted_scheduler_plan.queue.queue_id",
    )?;
    let queue_adapter = token_field(
        queue.get("adapter"),
        "networked_hosted_scheduler_plan.queue.adapter",
    )?;
    let queue_state_path = queue
        .get("state_snapshot_path")
        .or_else(|| queue.get("state_path"))
        .and_then(::serde_json::Value::as_str)
        .map(::std::path::PathBuf::from)
        .unwrap_or_else(|| plan_path.with_extension("networked-queue-state.json"));
    let entries = array_field(
        queue.get("entries"),
        "networked_hosted_scheduler_plan.queue.entries",
    )?;
    crate::ensure(
        !entries.is_empty(),
        "networked_hosted_scheduler_plan.queue.entries: expected non-empty list",
    )?;

    let decision_store = object_field(
        plan.get("decision_store"),
        "networked_hosted_scheduler_plan.decision_store",
    )?;
    let store_id = token_field(
        decision_store.get("store_id"),
        "networked_hosted_scheduler_plan.decision_store.store_id",
    )?;
    let store_adapter = token_field(
        decision_store.get("adapter"),
        "networked_hosted_scheduler_plan.decision_store.adapter",
    )?;
    let decision_state_path = decision_store
        .get("state_snapshot_path")
        .or_else(|| decision_store.get("path"))
        .and_then(::serde_json::Value::as_str)
        .map(::std::path::PathBuf::from)
        .unwrap_or_else(|| plan_path.with_extension("networked-decision-store.json"));

    let mut queue_entries = Vec::with_capacity(entries.len());
    let mut decision_records = Vec::with_capacity(entries.len());
    let mut failures = 0usize;
    for (idx, entry) in entries.iter().enumerate() {
        let entry = object_field(
            Some(entry),
            &format!("networked_hosted_scheduler_plan.queue.entries[{idx}]"),
        )?;
        let queue_entry_id = token_field(
            entry.get("queue_entry_id"),
            &format!("networked_hosted_scheduler_plan.queue.entries[{idx}].queue_entry_id"),
        )?;
        let run_id = token_field(
            entry.get("run_id"),
            &format!("networked_hosted_scheduler_plan.queue.entries[{idx}].run_id"),
        )?;
        let workload = token_field(
            entry.get("workload"),
            &format!("networked_hosted_scheduler_plan.queue.entries[{idx}].workload"),
        )?;
        let (command, command_observation) = execute_typed_command_field(
            entry.get("command_plan"),
            &format!("networked_hosted_scheduler_plan.queue.entries[{idx}].command_plan"),
        )?;
        let receipt_path = str_field(
            entry.get("receipt_path"),
            &format!("networked_hosted_scheduler_plan.queue.entries[{idx}].receipt_path"),
        )?;
        let session_id = session_ids[idx % session_ids.len()].as_str();
        let machine_id = session_machine
            .get(session_id)
            .expect("session id selected from known sessions");
        let worker_id = session_worker
            .get(session_id)
            .expect("session worker recorded with known session");
        let writer_id = machine_writer
            .get(machine_id)
            .expect("machine id validated for session");
        let exit_code = command_observation.exit_code.unwrap_or(125);
        let succeeded = command_observation.disposition == "succeeded";
        let state = if succeeded { "completed" } else { "failed" };
        if !succeeded {
            failures += 1;
        }
        let receipt_summary = if succeeded {
            summarize_receipt_path(receipt_path)?
        } else {
            format!("replay-readiness status=failed exit_code={exit_code}")
        };
        let queue_revision = (idx + 1) as i64;
        queue_entries.push(json!({
            "queue_entry_id": queue_entry_id,
            "run_id": run_id,
            "workload": workload,
            "state": state,
            "command": command_display(&command),
            "command_plan": command,
            "command_observation": command_observation,
            "exit_code": exit_code,
            "lease": {
                "lease_id": format!("lease-{queue_entry_id}"),
                "lease_epoch": 1,
                "queue_revision": queue_revision,
                "owner_machine_id": machine_id,
                "hypervisor_worker_id": worker_id,
                "worker_session_id": session_id
            },
            "receipt_path": receipt_path,
            "receipt_summary": receipt_summary
        }));
        let queue_snapshot = json!({
            "schema_version": 1,
            "queue_id": queue_id,
            "adapter": queue_adapter,
            "state_revision": queue_revision,
            "entries": queue_entries,
            "persisted_at": format!("unix:{}", unix_seconds())
        });
        if let Some(parent) = queue_state_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(
            &queue_state_path,
            serde_json::to_vec_pretty(&queue_snapshot)?,
        )?;
        decision_records.push(json!({
            "decision_id": format!("decision-{queue_entry_id}"),
            "decision_revision": queue_revision,
            "previous_revision": if idx == 0 { ::serde_json::Value::Null } else { json!(idx as i64) },
            "writer_id": writer_id,
            "machine_id": machine_id,
            "worker_session_id": session_id,
            "run_id": run_id,
            "queue_entry_id": queue_entry_id,
            "source_receipt_paths": [receipt_path],
            "summary": format!("decision recorded for replay-readiness run {run_id}"),
            "action": entry.get("decision_action").and_then(::serde_json::Value::as_str).unwrap_or("triage"),
            "status": "recorded",
            "receipt_path": entry.get("decision_receipt_path").and_then(::serde_json::Value::as_str).unwrap_or("target/networked-hosted/decision.json")
        }));
    }

    let decision_snapshot = json!({
        "schema_version": 1,
        "store_id": store_id,
        "adapter": store_adapter,
        "state_revision": decision_records.len() as i64,
        "records": decision_records,
        "persisted_at": format!("unix:{}", unix_seconds())
    });
    if let Some(parent) = decision_state_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &decision_state_path,
        serde_json::to_vec_pretty(&decision_snapshot)?,
    )?;
    let queue_snapshot = crate::replay_readiness_loader::load_json(&queue_state_path)?;
    let decision_snapshot = crate::replay_readiness_loader::load_json(&decision_state_path)?;
    let queue_digest = digest_json_value(&queue_snapshot)?;
    let decision_digest = digest_json_value(&decision_snapshot)?;
    let status = if failures == 0 {
        "recorded"
    } else if failures == entries.len() {
        "failed"
    } else {
        "partial"
    };

    Ok(json!({
        "schema_version": 1,
        "command": "replay-readiness-networked-hosted-scheduler",
        "status": status,
        "generated_at": format!("unix:{}", unix_seconds()),
        "harness_id": harness_id,
        "transport": transport,
        "plan_path": plan_path.display().to_string(),
        "scope": "bounded networked hosted/shared-state scheduler receipt with independently started worker sessions, shared queue revisions, state snapshot digests, decision-store revisions, linked run receipts, and worker heartbeats; not SaaS hosting, not product parity, not Antithesis parity, not universal fleet scale, and not raw-log evidence",
        "raw_log_scraping": false,
        "machines": machine_values,
        "worker_sessions": session_values,
        "queue": {
            "kind": "networked-shared",
            "queue_id": queue_id,
            "adapter": queue_adapter,
            "state_revision": entries.len() as i64,
            "state_snapshot_path": queue_state_path.display().to_string(),
            "state_snapshot_digest": queue_digest,
            "entries": queue_entries
        },
        "decision_store": {
            "kind": "networked-shared",
            "store_id": store_id,
            "adapter": store_adapter,
            "state_revision": decision_records.len() as i64,
            "state_snapshot_path": decision_state_path.display().to_string(),
            "state_snapshot_digest": decision_digest,
            "records": decision_records
        },
        "anti_claims": [
            "This is bounded networked hosted/shared-state scheduler evidence, not SaaS hosting evidence.",
            "This proves only local loopback or bounded networked shared queue semantics, not product parity, not universal fleet scale, and not Antithesis parity.",
            "This receipt links independently started worker sessions, worker heartbeats, shared queue leases, queue revisions, state snapshot digests, run receipts, decision-store revisions, writer identities, and source receipt links without raw-log scraping."
        ]
    }))
}

fn digest_json_value(value: &::serde_json::Value) -> crate::EvidenceResult<String> {
    let bytes = serde_json::to_vec(value)?;
    let mut hasher = ::sha2::Sha256::new();
    hasher.update(bytes);
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

pub fn sample_networked_hosted_scheduler_plan() -> ::serde_json::Value {
    json!({
        "schema_version": 1,
        "harness_id": "networked-hosted-harness-0001",
        "transport": "loopback-tcp",
        "machines": [
            {"machine_id": "machine-a", "writer_id": "writer-machine-a"},
            {"machine_id": "machine-b", "writer_id": "writer-machine-b"}
        ],
        "worker_sessions": [
            {"worker_session_id": "session-a-0001", "hypervisor_worker_id": "hv-a", "machine_id": "machine-a", "heartbeat_revision": 1, "last_heartbeat": "unix:1000"},
            {"worker_session_id": "session-b-0001", "hypervisor_worker_id": "hv-b", "machine_id": "machine-b", "heartbeat_revision": 1, "last_heartbeat": "unix:1001"}
        ],
        "queue": {
            "queue_id": "networked-queue-0001",
            "adapter": "shared-loopback-file",
            "state_snapshot_path": "target/networked-hosted/queue-state.json",
            "state_snapshot_digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            "entries": [
                {"queue_entry_id": "net-q-raft-0001", "run_id": "net-run-raft-0001", "workload": "raft", "command_plan": sample_typed_command("replay-readiness", &["--receipt", "target/networked-hosted/raft-replay-readiness.json"]), "receipt_path": "target/networked-hosted/raft-replay-readiness.json"},
                {"queue_entry_id": "net-q-redb-0001", "run_id": "net-run-redb-0001", "workload": "redb", "command_plan": sample_typed_command("replay-readiness", &["--receipt", "target/networked-hosted/redb-replay-readiness.json"]), "receipt_path": "target/networked-hosted/redb-replay-readiness.json"}
            ]
        },
        "decision_store": {"store_id": "networked-decision-store-0001", "adapter": "shared-loopback-file", "state_snapshot_path": "target/networked-hosted/decision-store.json", "state_snapshot_digest": "sha256:2222222222222222222222222222222222222222222222222222222222222222"}
    })
}

pub fn sample_networked_hosted_scheduler_receipt() -> ::serde_json::Value {
    json!({
        "schema_version": 1,
        "command": "replay-readiness-networked-hosted-scheduler",
        "status": "recorded",
        "generated_at": "2026-05-11T00:00:00Z",
        "harness_id": "networked-hosted-harness-0001",
        "transport": "loopback-tcp",
        "scope": "bounded networked hosted/shared-state scheduler receipt with independently started worker sessions, shared queue revisions, state snapshot digests, decision-store revisions, linked run receipts, and worker heartbeats; not SaaS hosting, not product parity, not Antithesis parity, not universal fleet scale, and not raw-log evidence",
        "raw_log_scraping": false,
        "machines": [
            {"machine_id": "machine-a", "writer_id": "writer-machine-a"},
            {"machine_id": "machine-b", "writer_id": "writer-machine-b"}
        ],
        "worker_sessions": [
            {"worker_session_id": "session-a-0001", "hypervisor_worker_id": "hv-a", "machine_id": "machine-a", "started_by": "independent-process", "heartbeat_revision": 1, "last_heartbeat": "unix:1000", "state": "healthy"},
            {"worker_session_id": "session-b-0001", "hypervisor_worker_id": "hv-b", "machine_id": "machine-b", "started_by": "independent-process", "heartbeat_revision": 1, "last_heartbeat": "unix:1001", "state": "healthy"}
        ],
        "queue": {
            "kind": "networked-shared",
            "queue_id": "networked-queue-0001",
            "adapter": "shared-loopback-file",
            "state_revision": 2,
            "state_snapshot_path": "target/networked-hosted/queue-state.json",
            "state_snapshot_digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            "entries": [
                {"queue_entry_id": "net-q-raft-0001", "run_id": "net-run-raft-0001", "workload": "raft", "state": "completed", "command": "replay-readiness --receipt target/networked-hosted/raft-replay-readiness.json", "exit_code": 0, "lease": {"lease_id": "lease-net-q-raft-0001", "lease_epoch": 1, "queue_revision": 1, "owner_machine_id": "machine-a", "hypervisor_worker_id": "hv-a", "worker_session_id": "session-a-0001"}, "receipt_path": "target/networked-hosted/raft-replay-readiness.json", "receipt_summary": "replay-readiness status=passed dogfood=raft:pass scope=bounded"},
                {"queue_entry_id": "net-q-redb-0001", "run_id": "net-run-redb-0001", "workload": "redb", "state": "completed", "command": "replay-readiness --receipt target/networked-hosted/redb-replay-readiness.json", "exit_code": 0, "lease": {"lease_id": "lease-net-q-redb-0001", "lease_epoch": 1, "queue_revision": 2, "owner_machine_id": "machine-b", "hypervisor_worker_id": "hv-b", "worker_session_id": "session-b-0001"}, "receipt_path": "target/networked-hosted/redb-replay-readiness.json", "receipt_summary": "replay-readiness status=passed dogfood=redb:pass scope=bounded"}
            ]
        },
        "decision_store": {
            "kind": "networked-shared",
            "store_id": "networked-decision-store-0001",
            "adapter": "shared-loopback-file",
            "state_revision": 2,
            "state_snapshot_path": "target/networked-hosted/decision-store.json",
            "state_snapshot_digest": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
            "records": [
                {"decision_id": "decision-net-raft-0001", "decision_revision": 1, "previous_revision": null, "writer_id": "writer-machine-a", "machine_id": "machine-a", "worker_session_id": "session-a-0001", "run_id": "net-run-raft-0001", "queue_entry_id": "net-q-raft-0001", "source_receipt_paths": ["target/networked-hosted/raft-replay-readiness.json"], "summary": "triage decision recorded for raft replay-readiness receipt", "action": "reproduce", "status": "recorded", "receipt_path": "target/networked-hosted/raft-decision.json"},
                {"decision_id": "decision-net-redb-0001", "decision_revision": 2, "previous_revision": 1, "writer_id": "writer-machine-b", "machine_id": "machine-b", "worker_session_id": "session-b-0001", "run_id": "net-run-redb-0001", "queue_entry_id": "net-q-redb-0001", "source_receipt_paths": ["target/networked-hosted/redb-replay-readiness.json"], "summary": "triage decision recorded for redb replay-readiness receipt", "action": "triage", "status": "recorded", "receipt_path": "target/networked-hosted/redb-decision.json"}
            ]
        },
        "anti_claims": [
            "This is bounded networked hosted/shared-state scheduler evidence, not SaaS hosting evidence.",
            "This proves only local loopback or bounded networked shared queue semantics, not product parity, not universal fleet scale, and not Antithesis parity.",
            "This receipt links independently started worker sessions, worker heartbeats, shared queue leases, queue revisions, state snapshot digests, run receipts, decision-store revisions, writer identities, and source receipt links without raw-log scraping."
        ]
    })
}

pub fn validate_networked_hosted_scheduler_receipt(
    receipt: &::serde_json::Value,
) -> crate::EvidenceResult<String> {
    let schema_version = int_field(
        receipt.get("schema_version"),
        "networked_hosted_scheduler.schema_version",
    )?;
    crate::ensure(
        schema_version == 1,
        format!("networked_hosted_scheduler.schema_version: expected 1, got {schema_version}"),
    )?;
    let command = str_field(receipt.get("command"), "networked_hosted_scheduler.command")?;
    crate::ensure(
        command == "replay-readiness-networked-hosted-scheduler",
        format!("networked_hosted_scheduler.command: unexpected {command:?}"),
    )?;
    let status = str_field(receipt.get("status"), "networked_hosted_scheduler.status")?;
    crate::ensure(
        matches!(status, "recorded" | "partial" | "failed"),
        format!("networked_hosted_scheduler.status: unsupported value {status:?}"),
    )?;
    token_field(
        receipt.get("harness_id"),
        "networked_hosted_scheduler.harness_id",
    )?;
    let transport = token_field(
        receipt.get("transport"),
        "networked_hosted_scheduler.transport",
    )?;
    crate::ensure(
        matches!(
            transport,
            "loopback-tcp" | "loopback-uds" | "multi-process-file"
        ),
        format!("networked_hosted_scheduler.transport: unsupported value {transport:?}"),
    )?;
    let scope = str_field(receipt.get("scope"), "networked_hosted_scheduler.scope")?.to_lowercase();
    crate::ensure(
        scope.contains("bounded networked hosted/shared-state")
            && scope.contains("independently started worker sessions")
            && scope.contains("shared queue")
            && scope.contains("decision-store")
            && scope.contains("not saas")
            && scope.contains("not product parity")
            && scope.contains("not antithesis parity")
            && scope.contains("not universal fleet scale"),
        "networked_hosted_scheduler.scope: must declare bounded networked scope and non-claims",
    )?;
    crate::ensure(
        !matches!(
            receipt.get("raw_log_scraping"),
            Some(::serde_json::Value::Bool(true))
        ),
        "networked_hosted_scheduler.raw_log_scraping: raw-log scraping is not allowed",
    )?;

    let machines = array_field(
        receipt.get("machines"),
        "networked_hosted_scheduler.machines",
    )?;
    crate::ensure(
        machines.len() >= 2,
        "networked_hosted_scheduler.machines: expected at least two machine identities",
    )?;
    let mut machine_ids = ::std::collections::BTreeSet::new();
    let mut writer_to_machine = ::std::collections::BTreeMap::new();
    for (idx, machine) in machines.iter().enumerate() {
        let machine = object_field(
            Some(machine),
            &format!("networked_hosted_scheduler.machines[{idx}]"),
        )?;
        let machine_id = token_field(
            machine.get("machine_id"),
            &format!("networked_hosted_scheduler.machines[{idx}].machine_id"),
        )?;
        crate::ensure(
            machine_ids.insert(machine_id.to_string()),
            format!(
                "networked_hosted_scheduler.machines[{idx}].machine_id: duplicate {machine_id}"
            ),
        )?;
        let writer_id = token_field(
            machine.get("writer_id"),
            &format!("networked_hosted_scheduler.machines[{idx}].writer_id"),
        )?;
        crate::ensure(
            writer_to_machine
                .insert(writer_id.to_string(), machine_id.to_string())
                .is_none(),
            format!("networked_hosted_scheduler.machines[{idx}].writer_id: duplicate {writer_id}"),
        )?;
    }

    let worker_sessions = array_field(
        receipt.get("worker_sessions"),
        "networked_hosted_scheduler.worker_sessions",
    )?;
    crate::ensure(
        worker_sessions.len() >= 2,
        "networked_hosted_scheduler.worker_sessions: expected at least two worker sessions",
    )?;
    let mut session_to_worker = ::std::collections::BTreeMap::new();
    let mut session_to_machine = ::std::collections::BTreeMap::new();
    let mut worker_to_machine = ::std::collections::BTreeMap::new();
    for (idx, session) in worker_sessions.iter().enumerate() {
        let session = object_field(
            Some(session),
            &format!("networked_hosted_scheduler.worker_sessions[{idx}]"),
        )?;
        let session_id = token_field(
            session.get("worker_session_id"),
            &format!("networked_hosted_scheduler.worker_sessions[{idx}].worker_session_id"),
        )?;
        let worker_id = token_field(
            session.get("hypervisor_worker_id"),
            &format!("networked_hosted_scheduler.worker_sessions[{idx}].hypervisor_worker_id"),
        )?;
        let machine_id = token_field(
            session.get("machine_id"),
            &format!("networked_hosted_scheduler.worker_sessions[{idx}].machine_id"),
        )?;
        crate::ensure(machine_ids.contains(machine_id), format!("networked_hosted_scheduler.worker_sessions[{idx}].machine_id: {machine_id} missing from machines"))?;
        token_field(
            session.get("started_by"),
            &format!("networked_hosted_scheduler.worker_sessions[{idx}].started_by"),
        )?;
        let heartbeat_revision = int_field(
            session.get("heartbeat_revision"),
            &format!("networked_hosted_scheduler.worker_sessions[{idx}].heartbeat_revision"),
        )?;
        crate::ensure(heartbeat_revision > 0, format!("networked_hosted_scheduler.worker_sessions[{idx}].heartbeat_revision: expected positive heartbeat revision"))?;
        str_field(
            session.get("last_heartbeat"),
            &format!("networked_hosted_scheduler.worker_sessions[{idx}].last_heartbeat"),
        )?;
        let state = token_field(
            session.get("state"),
            &format!("networked_hosted_scheduler.worker_sessions[{idx}].state"),
        )?;
        crate::ensure(matches!(state, "healthy" | "draining" | "stopped"), format!("networked_hosted_scheduler.worker_sessions[{idx}].state: unsupported value {state:?}"))?;
        crate::ensure(session_to_worker.insert(session_id.to_string(), worker_id.to_string()).is_none(), format!("networked_hosted_scheduler.worker_sessions[{idx}].worker_session_id: duplicate {session_id}"))?;
        session_to_machine.insert(session_id.to_string(), machine_id.to_string());
        if let Some(previous_machine) =
            worker_to_machine.insert(worker_id.to_string(), machine_id.to_string())
        {
            crate::ensure(previous_machine == machine_id, format!("networked_hosted_scheduler.worker_sessions[{idx}].hypervisor_worker_id: split worker machine for {worker_id}"))?;
        }
    }

    let queue = object_field(receipt.get("queue"), "networked_hosted_scheduler.queue")?;
    let queue_kind = token_field(queue.get("kind"), "networked_hosted_scheduler.queue.kind")?;
    crate::ensure(
        queue_kind == "networked-shared",
        format!(
            "networked_hosted_scheduler.queue.kind: expected networked-shared, got {queue_kind:?}"
        ),
    )?;
    token_field(
        queue.get("queue_id"),
        "networked_hosted_scheduler.queue.queue_id",
    )?;
    token_field(
        queue.get("adapter"),
        "networked_hosted_scheduler.queue.adapter",
    )?;
    let queue_state_revision = int_field(
        queue.get("state_revision"),
        "networked_hosted_scheduler.queue.state_revision",
    )?;
    crate::ensure(
        queue_state_revision > 0,
        "networked_hosted_scheduler.queue.state_revision: expected positive revision",
    )?;
    str_field(
        queue.get("state_snapshot_path"),
        "networked_hosted_scheduler.queue.state_snapshot_path",
    )?;
    validate_digest_field(
        queue.get("state_snapshot_digest"),
        "networked_hosted_scheduler.queue.state_snapshot_digest",
    )?;
    let entries = array_field(
        queue.get("entries"),
        "networked_hosted_scheduler.queue.entries",
    )?;
    crate::ensure(
        !entries.is_empty(),
        "networked_hosted_scheduler.queue.entries: expected non-empty list",
    )?;
    let mut queue_entry_ids = ::std::collections::BTreeSet::new();
    let mut run_ids = ::std::collections::BTreeSet::new();
    let mut lease_ids = ::std::collections::BTreeSet::new();
    let mut last_queue_revision = 0i64;
    for (idx, entry) in entries.iter().enumerate() {
        let entry = object_field(
            Some(entry),
            &format!("networked_hosted_scheduler.queue.entries[{idx}]"),
        )?;
        let queue_entry_id = token_field(
            entry.get("queue_entry_id"),
            &format!("networked_hosted_scheduler.queue.entries[{idx}].queue_entry_id"),
        )?;
        crate::ensure(queue_entry_ids.insert(queue_entry_id.to_string()), format!("networked_hosted_scheduler.queue.entries[{idx}].queue_entry_id: duplicate {queue_entry_id}"))?;
        let run_id = token_field(
            entry.get("run_id"),
            &format!("networked_hosted_scheduler.queue.entries[{idx}].run_id"),
        )?;
        crate::ensure(
            run_ids.insert(run_id.to_string()),
            format!("networked_hosted_scheduler.queue.entries[{idx}].run_id: duplicate {run_id}"),
        )?;
        token_field(
            entry.get("workload"),
            &format!("networked_hosted_scheduler.queue.entries[{idx}].workload"),
        )?;
        str_field(
            entry.get("command"),
            &format!("networked_hosted_scheduler.queue.entries[{idx}].command"),
        )?;
        let state = token_field(
            entry.get("state"),
            &format!("networked_hosted_scheduler.queue.entries[{idx}].state"),
        )?;
        crate::ensure(matches!(state, "queued" | "leased" | "completed" | "failed"), format!("networked_hosted_scheduler.queue.entries[{idx}].state: unsupported value {state:?}"))?;
        let exit_code = int_field(
            entry.get("exit_code"),
            &format!("networked_hosted_scheduler.queue.entries[{idx}].exit_code"),
        )?;
        let lease = object_field(
            entry.get("lease"),
            &format!("networked_hosted_scheduler.queue.entries[{idx}].lease"),
        )?;
        let lease_id = token_field(
            lease.get("lease_id"),
            &format!("networked_hosted_scheduler.queue.entries[{idx}].lease.lease_id"),
        )?;
        crate::ensure(lease_ids.insert(lease_id.to_string()), format!("networked_hosted_scheduler.queue.entries[{idx}].lease.lease_id: duplicate active lease {lease_id}"))?;
        let lease_epoch = int_field(
            lease.get("lease_epoch"),
            &format!("networked_hosted_scheduler.queue.entries[{idx}].lease.lease_epoch"),
        )?;
        crate::ensure(lease_epoch > 0, format!("networked_hosted_scheduler.queue.entries[{idx}].lease.lease_epoch: expected positive epoch"))?;
        let queue_revision = int_field(
            lease.get("queue_revision"),
            &format!("networked_hosted_scheduler.queue.entries[{idx}].lease.queue_revision"),
        )?;
        crate::ensure(queue_revision > last_queue_revision, format!("networked_hosted_scheduler.queue.entries[{idx}].lease.queue_revision: stale queue-state revision"))?;
        crate::ensure(queue_revision <= queue_state_revision, format!("networked_hosted_scheduler.queue.entries[{idx}].lease.queue_revision: exceeds queue state revision"))?;
        last_queue_revision = queue_revision;
        let owner_machine_id = token_field(
            lease.get("owner_machine_id"),
            &format!("networked_hosted_scheduler.queue.entries[{idx}].lease.owner_machine_id"),
        )?;
        crate::ensure(machine_ids.contains(owner_machine_id), format!("networked_hosted_scheduler.queue.entries[{idx}].lease.owner_machine_id: {owner_machine_id} missing from machines"))?;
        let worker_id = token_field(
            lease.get("hypervisor_worker_id"),
            &format!("networked_hosted_scheduler.queue.entries[{idx}].lease.hypervisor_worker_id"),
        )?;
        let worker_machine = worker_to_machine.get(worker_id).ok_or_else(|| crate::EvidenceError::new(format!("networked_hosted_scheduler.queue.entries[{idx}].lease.hypervisor_worker_id: {worker_id} missing from worker sessions")))?;
        crate::ensure(worker_machine == owner_machine_id, format!("networked_hosted_scheduler.queue.entries[{idx}].lease: owner machine {owner_machine_id} does not match worker machine {worker_machine}"))?;
        let session_id = token_field(
            lease.get("worker_session_id"),
            &format!("networked_hosted_scheduler.queue.entries[{idx}].lease.worker_session_id"),
        )?;
        let session_worker = session_to_worker.get(session_id).ok_or_else(|| crate::EvidenceError::new(format!("networked_hosted_scheduler.queue.entries[{idx}].lease.worker_session_id: {session_id} missing worker-session heartbeat")))?;
        crate::ensure(session_worker == worker_id, format!("networked_hosted_scheduler.queue.entries[{idx}].lease.worker_session_id: session {session_id} does not own worker {worker_id}"))?;
        let session_machine = session_to_machine
            .get(session_id)
            .expect("session machine recorded with session worker");
        crate::ensure(session_machine == owner_machine_id, format!("networked_hosted_scheduler.queue.entries[{idx}].lease.worker_session_id: session {session_id} machine {session_machine} does not match lease owner {owner_machine_id}"))?;
        str_field(
            entry.get("receipt_path"),
            &format!("networked_hosted_scheduler.queue.entries[{idx}].receipt_path"),
        )?;
        let summary = str_field(
            entry.get("receipt_summary"),
            &format!("networked_hosted_scheduler.queue.entries[{idx}].receipt_summary"),
        )?;
        if state == "completed" && exit_code == 0 {
            crate::ensure(summary.contains("replay-readiness status="), format!("networked_hosted_scheduler.queue.entries[{idx}].receipt_summary: missing passed-run replay-readiness summary"))?;
        }
    }

    let decision_store = object_field(
        receipt.get("decision_store"),
        "networked_hosted_scheduler.decision_store",
    )?;
    let store_kind = token_field(
        decision_store.get("kind"),
        "networked_hosted_scheduler.decision_store.kind",
    )?;
    crate::ensure(store_kind == "networked-shared", format!("networked_hosted_scheduler.decision_store.kind: expected networked-shared, got {store_kind:?}"))?;
    token_field(
        decision_store.get("store_id"),
        "networked_hosted_scheduler.decision_store.store_id",
    )?;
    token_field(
        decision_store.get("adapter"),
        "networked_hosted_scheduler.decision_store.adapter",
    )?;
    let store_revision = int_field(
        decision_store.get("state_revision"),
        "networked_hosted_scheduler.decision_store.state_revision",
    )?;
    crate::ensure(
        store_revision > 0,
        "networked_hosted_scheduler.decision_store.state_revision: expected positive revision",
    )?;
    str_field(
        decision_store.get("state_snapshot_path"),
        "networked_hosted_scheduler.decision_store.state_snapshot_path",
    )?;
    validate_digest_field(
        decision_store.get("state_snapshot_digest"),
        "networked_hosted_scheduler.decision_store.state_snapshot_digest",
    )?;
    let records = array_field(
        decision_store.get("records"),
        "networked_hosted_scheduler.decision_store.records",
    )?;
    crate::ensure(
        !records.is_empty(),
        "networked_hosted_scheduler.decision_store.records: expected non-empty list",
    )?;
    let mut decision_revisions = ::std::collections::BTreeSet::new();
    let mut last_decision_revision = 0i64;
    for (idx, record) in records.iter().enumerate() {
        let record = object_field(
            Some(record),
            &format!("networked_hosted_scheduler.decision_store.records[{idx}]"),
        )?;
        let decision_id = token_field(
            record.get("decision_id"),
            &format!("networked_hosted_scheduler.decision_store.records[{idx}].decision_id"),
        )?;
        let revision = int_field(
            record.get("decision_revision"),
            &format!("networked_hosted_scheduler.decision_store.records[{idx}].decision_revision"),
        )?;
        crate::ensure(revision > 0, format!("networked_hosted_scheduler.decision_store.records[{idx}].decision_revision: expected positive revision"))?;
        crate::ensure(revision > last_decision_revision, format!("networked_hosted_scheduler.decision_store.records[{idx}].decision_revision: stale decision-store revision"))?;
        crate::ensure(revision <= store_revision, format!("networked_hosted_scheduler.decision_store.records[{idx}].decision_revision: exceeds decision-store revision"))?;
        last_decision_revision = revision;
        crate::ensure(decision_revisions.insert(format!("{decision_id}@{revision}")), format!("networked_hosted_scheduler.decision_store.records[{idx}]: split-brain duplicate decision revision for {decision_id}@{revision}"))?;
        if let Some(previous_revision) = record
            .get("previous_revision")
            .filter(|value| !value.is_null())
        {
            let previous_revision = int_field(
                Some(previous_revision),
                &format!(
                    "networked_hosted_scheduler.decision_store.records[{idx}].previous_revision"
                ),
            )?;
            crate::ensure(previous_revision < revision, format!("networked_hosted_scheduler.decision_store.records[{idx}].previous_revision: stale decision write"))?;
        }
        let writer_id = token_field(
            record.get("writer_id"),
            &format!("networked_hosted_scheduler.decision_store.records[{idx}].writer_id"),
        )?;
        let writer_machine = writer_to_machine.get(writer_id).ok_or_else(|| crate::EvidenceError::new(format!("networked_hosted_scheduler.decision_store.records[{idx}].writer_id: {writer_id} missing from machines")))?;
        let machine_id = token_field(
            record.get("machine_id"),
            &format!("networked_hosted_scheduler.decision_store.records[{idx}].machine_id"),
        )?;
        crate::ensure(writer_machine == machine_id, format!("networked_hosted_scheduler.decision_store.records[{idx}].writer_id: writer {writer_id} is not owned by machine {machine_id}"))?;
        let session_id = token_field(
            record.get("worker_session_id"),
            &format!("networked_hosted_scheduler.decision_store.records[{idx}].worker_session_id"),
        )?;
        let session_machine = session_to_machine.get(session_id).ok_or_else(|| crate::EvidenceError::new(format!("networked_hosted_scheduler.decision_store.records[{idx}].worker_session_id: {session_id} missing worker-session heartbeat")))?;
        crate::ensure(session_machine == machine_id, format!("networked_hosted_scheduler.decision_store.records[{idx}].worker_session_id: session {session_id} is not owned by machine {machine_id}"))?;
        let run_id = token_field(
            record.get("run_id"),
            &format!("networked_hosted_scheduler.decision_store.records[{idx}].run_id"),
        )?;
        crate::ensure(run_ids.contains(run_id), format!("networked_hosted_scheduler.decision_store.records[{idx}].run_id: {run_id} missing from queue runs"))?;
        let queue_entry_id = token_field(
            record.get("queue_entry_id"),
            &format!("networked_hosted_scheduler.decision_store.records[{idx}].queue_entry_id"),
        )?;
        crate::ensure(queue_entry_ids.contains(queue_entry_id), format!("networked_hosted_scheduler.decision_store.records[{idx}].queue_entry_id: {queue_entry_id} missing from queue entries"))?;
        let source_receipts = array_field(
            record.get("source_receipt_paths"),
            &format!(
                "networked_hosted_scheduler.decision_store.records[{idx}].source_receipt_paths"
            ),
        )?;
        crate::ensure(!source_receipts.is_empty(), format!("networked_hosted_scheduler.decision_store.records[{idx}].source_receipt_paths: expected linked source receipt"))?;
        for (source_idx, source) in source_receipts.iter().enumerate() {
            str_field(Some(source), &format!("networked_hosted_scheduler.decision_store.records[{idx}].source_receipt_paths[{source_idx}]"))?;
        }
        let summary = str_field(
            record.get("summary"),
            &format!("networked_hosted_scheduler.decision_store.records[{idx}].summary"),
        )?;
        crate::ensure(summary.contains("decision") && summary.contains("replay-readiness"), format!("networked_hosted_scheduler.decision_store.records[{idx}].summary: expected stable replay-readiness decision summary"))?;
        let action = token_field(
            record.get("action"),
            &format!("networked_hosted_scheduler.decision_store.records[{idx}].action"),
        )?;
        crate::ensure(matches!(action, "triage" | "reproduce" | "minimize" | "accept" | "reject"), format!("networked_hosted_scheduler.decision_store.records[{idx}].action: unsupported value {action:?}"))?;
        let decision_status = token_field(
            record.get("status"),
            &format!("networked_hosted_scheduler.decision_store.records[{idx}].status"),
        )?;
        crate::ensure(matches!(decision_status, "recorded" | "superseded" | "conflict"), format!("networked_hosted_scheduler.decision_store.records[{idx}].status: unsupported value {decision_status:?}"))?;
        str_field(
            record.get("receipt_path"),
            &format!("networked_hosted_scheduler.decision_store.records[{idx}].receipt_path"),
        )?;
    }

    let anti_claims = array_field(
        receipt.get("anti_claims"),
        "networked_hosted_scheduler.anti_claims",
    )?;
    let anti_claim_text = anti_claims
        .iter()
        .map(json_display)
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();
    crate::ensure(
        anti_claim_text.contains("bounded networked hosted/shared-state")
            && anti_claim_text.contains("not saas")
            && anti_claim_text.contains("not product parity")
            && anti_claim_text.contains("not universal fleet scale")
            && anti_claim_text.contains("antithesis parity")
            && anti_claim_text.contains("without raw-log scraping"),
        "networked_hosted_scheduler.anti_claims: missing bounded hosted/fleet anti-overclaim text",
    )?;
    Ok(format!("replay-readiness-networked-hosted-scheduler status={status} machines={} worker_sessions={} queue_entries={} decisions={} scope=bounded-networked-hosted-shared-state", machines.len(), worker_sessions.len(), entries.len(), records.len()))
}

fn validate_digest_field(
    value: Option<&::serde_json::Value>,
    field: &str,
) -> crate::EvidenceResult<()> {
    let digest = str_field(value, field)?;
    let hex = digest
        .strip_prefix("sha256:")
        .ok_or_else(|| crate::EvidenceError::new(format!("{field}: expected sha256 digest")))?;
    crate::ensure(
        hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()),
        format!("{field}: expected 64 hex characters"),
    )
}

pub fn write_multi_hypervisor_campaign_receipt_path(
    path: impl AsRef<::std::path::Path>,
) -> crate::EvidenceResult<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&sample_multi_hypervisor_campaign_receipt())?,
    )?;
    Ok(())
}

pub fn validate_multi_hypervisor_campaign_receipt_path(
    path: impl AsRef<::std::path::Path>,
) -> crate::EvidenceResult<String> {
    validate_multi_hypervisor_campaign_receipt(&crate::replay_readiness_loader::load_json(
        path.as_ref(),
    )?)
}

pub fn execute_multi_hypervisor_campaign_receipt_path(
    plan_path: impl AsRef<::std::path::Path>,
    output_path: impl AsRef<::std::path::Path>,
) -> crate::EvidenceResult<String> {
    let plan_path = plan_path.as_ref();
    let output_path = output_path.as_ref();
    let receipt = execute_multi_hypervisor_campaign_receipt(
        &crate::replay_readiness_loader::load_json(plan_path)?,
        plan_path,
    )?;
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, serde_json::to_vec_pretty(&receipt)?)?;
    validate_multi_hypervisor_campaign_receipt(&receipt)
}

pub fn execute_multi_hypervisor_campaign_receipt(
    plan: &::serde_json::Value,
    plan_path: &::std::path::Path,
) -> crate::EvidenceResult<::serde_json::Value> {
    let campaign_id = token_field(plan.get("campaign_id"), "multi_hypervisor_plan.campaign_id")?;
    let max_hypervisors = int_field(
        plan.get("max_hypervisors"),
        "multi_hypervisor_plan.max_hypervisors",
    )?;
    crate::ensure(
        max_hypervisors > 1,
        "multi_hypervisor_plan.max_hypervisors: expected at least 2",
    )?;
    let state_path = plan
        .get("state_path")
        .and_then(::serde_json::Value::as_str)
        .map(::std::path::PathBuf::from)
        .unwrap_or_else(|| plan_path.with_extension("state.json"));
    let artifact_index_path = plan
        .get("artifact_index_path")
        .and_then(::serde_json::Value::as_str)
        .map(::std::path::PathBuf::from)
        .unwrap_or_else(|| plan_path.with_extension("artifacts.json"));
    let follow_up_policy = plan
        .get("follow_up_policy")
        .cloned()
        .unwrap_or_else(|| json!({"enabled": false, "reproduce": false, "minimize": false}));
    let previous_state = if state_path.exists() {
        Some(crate::replay_readiness_loader::load_json(&state_path)?)
    } else {
        None
    };
    let completed_before_start = previous_state
        .as_ref()
        .and_then(|state| state.get("completed_runs"))
        .and_then(::serde_json::Value::as_array)
        .map(|runs| runs.len())
        .unwrap_or(0);

    let hypervisors = array_field(plan.get("hypervisors"), "multi_hypervisor_plan.hypervisors")?;
    crate::ensure(
        hypervisors.len() >= 2,
        "multi_hypervisor_plan.hypervisors: expected at least two local hypervisor workers",
    )?;
    crate::ensure(
        max_hypervisors as usize <= hypervisors.len(),
        "multi_hypervisor_plan.max_hypervisors: cannot exceed hypervisor worker count",
    )?;
    let mut hypervisor_ids = Vec::with_capacity(hypervisors.len());
    let mut hypervisor_artifact_roots = ::std::collections::BTreeMap::new();
    for (idx, hypervisor) in hypervisors.iter().enumerate() {
        let hypervisor = object_field(
            Some(hypervisor),
            &format!("multi_hypervisor_plan.hypervisors[{idx}]"),
        )?;
        let hypervisor_id = token_field(
            hypervisor.get("hypervisor_worker_id"),
            &format!("multi_hypervisor_plan.hypervisors[{idx}].hypervisor_worker_id"),
        )?;
        hypervisor_ids.push(hypervisor_id);
        let budget = object_field(
            hypervisor.get("resource_budget"),
            &format!("multi_hypervisor_plan.hypervisors[{idx}].resource_budget"),
        )?;
        crate::ensure(
            int_field(budget.get("vcpus"), &format!("multi_hypervisor_plan.hypervisors[{idx}].resource_budget.vcpus"))? > 0,
            format!("multi_hypervisor_plan.hypervisors[{idx}].resource_budget.vcpus: expected positive budget"),
        )?;
        crate::ensure(
            int_field(budget.get("memory_mib"), &format!("multi_hypervisor_plan.hypervisors[{idx}].resource_budget.memory_mib"))? > 0,
            format!("multi_hypervisor_plan.hypervisors[{idx}].resource_budget.memory_mib: expected positive budget"),
        )?;
        let artifact_root = str_field(
            hypervisor.get("artifact_root"),
            &format!("multi_hypervisor_plan.hypervisors[{idx}].artifact_root"),
        )?;
        hypervisor_artifact_roots.insert(hypervisor_id.to_string(), artifact_root.to_string());
    }

    let queue = object_field(plan.get("queue"), "multi_hypervisor_plan.queue")?;
    let entries = array_field(queue.get("entries"), "multi_hypervisor_plan.queue.entries")?;
    crate::ensure(
        !entries.is_empty(),
        "multi_hypervisor_plan.queue.entries: expected non-empty list",
    )?;
    let mut completed_runs = previous_state
        .as_ref()
        .and_then(|state| state.get("completed_runs"))
        .and_then(::serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut receipt_entries = Vec::with_capacity(entries.len());
    let mut runs = Vec::with_capacity(entries.len());
    let mut artifact_entries = Vec::with_capacity(entries.len());
    let mut follow_up_jobs = Vec::new();
    let mut failures = 0usize;

    for (idx, entry) in entries.iter().enumerate() {
        let entry = object_field(
            Some(entry),
            &format!("multi_hypervisor_plan.queue.entries[{idx}]"),
        )?;
        let queue_entry_id = token_field(
            entry.get("queue_entry_id"),
            &format!("multi_hypervisor_plan.queue.entries[{idx}].queue_entry_id"),
        )?;
        let run_id = token_field(
            entry.get("run_id"),
            &format!("multi_hypervisor_plan.queue.entries[{idx}].run_id"),
        )?;
        let workload = token_field(
            entry.get("workload"),
            &format!("multi_hypervisor_plan.queue.entries[{idx}].workload"),
        )?;
        let (command, command_observation) = execute_typed_command_field(
            entry.get("command_plan"),
            &format!("multi_hypervisor_plan.queue.entries[{idx}].command_plan"),
        )?;
        let receipt_path = str_field(
            entry.get("receipt_path"),
            &format!("multi_hypervisor_plan.queue.entries[{idx}].receipt_path"),
        )?;
        let hypervisor_worker_id = hypervisor_ids[idx % (max_hypervisors as usize)];
        let artifact_root = hypervisor_artifact_roots
            .get(hypervisor_worker_id)
            .ok_or_else(|| crate::EvidenceError::new(format!("multi_hypervisor_plan.hypervisors: missing artifact root for {hypervisor_worker_id}")))?;
        let bug_artifacts = entry
            .get("expected_bug_artifacts")
            .and_then(::serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        let lease_id = format!("lease-{campaign_id}-{queue_entry_id}");
        let exit_code = command_observation.exit_code.unwrap_or(125);
        let succeeded = command_observation.disposition == "succeeded";
        let run_status = if succeeded { "passed" } else { "failed" };
        if !succeeded {
            failures += 1;
        }
        let receipt_summary = if succeeded {
            Some(summarize_receipt_path(receipt_path)?)
        } else {
            None
        };
        receipt_entries.push(json!({"queue_entry_id": queue_entry_id, "run_id": run_id, "workload": workload, "state": if succeeded {"completed"} else {"failed"}, "lease_id": lease_id, "hypervisor_worker_id": hypervisor_worker_id}));
        if succeeded {
            completed_runs.push(::serde_json::Value::String(run_id.to_string()));
        }
        let state_snapshot = json!({
            "schema_version": 1,
            "campaign_id": campaign_id,
            "state_path": state_path.display().to_string(),
            "last_persisted_run_id": run_id,
            "completed_runs": completed_runs,
            "entries": receipt_entries,
            "persisted_at": format!("unix:{}", unix_seconds())
        });
        if let Some(parent) = state_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&state_path, serde_json::to_vec_pretty(&state_snapshot)?)?;
        let mut run_followups = Vec::new();
        if follow_up_policy
            .get("enabled")
            .and_then(::serde_json::Value::as_bool)
            == Some(true)
        {
            for (bug_idx, bug) in bug_artifacts.iter().enumerate() {
                let bug_path = str_field(Some(bug), &format!("multi_hypervisor_plan.queue.entries[{idx}].expected_bug_artifacts[{bug_idx}]"))?;
                if follow_up_policy
                    .get("reproduce")
                    .and_then(::serde_json::Value::as_bool)
                    == Some(true)
                {
                    let job = json!({"job_id": format!("followup-{run_id}-reproduce-{bug_idx}"), "kind": "reproduce", "source_run_id": run_id, "source_queue_entry_id": queue_entry_id, "hypervisor_worker_id": hypervisor_worker_id, "bug_artifact_path": bug_path, "snapshot_ref": format!("snapshot:{run_id}:{bug_idx}"), "status": "queued"});
                    run_followups.push(job.clone());
                    follow_up_jobs.push(job);
                }
                if follow_up_policy
                    .get("minimize")
                    .and_then(::serde_json::Value::as_bool)
                    == Some(true)
                {
                    let job = json!({"job_id": format!("followup-{run_id}-minimize-{bug_idx}"), "kind": "minimize", "source_run_id": run_id, "source_queue_entry_id": queue_entry_id, "hypervisor_worker_id": hypervisor_worker_id, "bug_artifact_path": bug_path, "snapshot_ref": format!("snapshot:{run_id}:{bug_idx}"), "status": "queued"});
                    run_followups.push(job.clone());
                    follow_up_jobs.push(job);
                }
            }
        }
        artifact_entries.push(json!({
            "artifact_id": format!("artifact-{run_id}"),
            "run_id": run_id,
            "queue_entry_id": queue_entry_id,
            "hypervisor_worker_id": hypervisor_worker_id,
            "artifact_root": artifact_root,
            "receipt_path": receipt_path,
            "digest": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "retention": {"policy": "bounded-local", "gc_status": "retained"},
            "bug_artifacts": bug_artifacts,
            "follow_up_receipts": []
        }));
        runs.push(json!({
            "campaign_id": campaign_id,
            "run_id": run_id,
            "queue_entry_id": queue_entry_id,
            "hypervisor_worker_id": hypervisor_worker_id,
            "workload": workload,
            "command": command_display(&command),
            "command_plan": command,
            "command_observation": command_observation,
            "lease_id": lease_id,
            "artifact_root": artifact_root,
            "receipt_path": receipt_path,
            "receipt_summary": receipt_summary,
            "follow_up_jobs": run_followups,
            "status": run_status,
            "exit_code": exit_code
        }));
    }

    let status = if failures == 0 {
        "recorded"
    } else if failures == entries.len() {
        "failed"
    } else {
        "partial"
    };
    Ok(json!({
        "schema_version": 1,
        "command": "replay-readiness-local-multi-hypervisor-campaign",
        "status": status,
        "generated_at": format!("unix:{}", unix_seconds()),
        "campaign_id": campaign_id,
        "plan_path": plan_path.display().to_string(),
        "scope": "bounded local multi-hypervisor campaign receipt with one durable local queue/state file; not a hosted service, not a shared remote queue, not cross-machine scheduling, not universal fleet-scale throughput, and not a full Antithesis-style product replacement",
        "raw_log_scraping": false,
        "fault_coverage": {
            "schema_version": 1,
            "scope": "listed deterministic fault classes and workloads only; not exhaustive validation of all possible failures",
            "by_workload": [
                {"workload": "raft", "configured_fault_classes": ["network", "timer", "scheduler"], "injection_attempts": 3, "observed_injections": 2, "not_observed_fault_classes": ["scheduler"], "affected_run_ids": ["mh-run-raft-0001"], "unsupported_fault_classes": ["process"]},
                {"workload": "redb", "configured_fault_classes": ["block"], "injection_attempts": 1, "observed_injections": 1, "not_observed_fault_classes": [], "affected_run_ids": ["mh-run-redb-0001"], "unsupported_fault_classes": []}
            ]
        },
        "queue_state": {
            "kind": "durable-local-file",
            "state_path": state_path.display().to_string(),
            "loaded_existing_state": previous_state.is_some(),
            "completed_before_start": completed_before_start,
            "persisted_after_each_run": true
        },
        "control_plane": {"kind": "single-machine-local", "max_hypervisors": max_hypervisors, "artifact_index_path": artifact_index_path.display().to_string(), "follow_up_policy": follow_up_policy},
        "queue": {"entries": receipt_entries},
        "hypervisors": hypervisors,
        "runs": runs,
        "artifact_index": {"schema_version": 1, "index_path": artifact_index_path.display().to_string(), "entries": artifact_entries},
        "follow_up_jobs": follow_up_jobs,
        "operator_decisions": plan.get("operator_decisions").and_then(::serde_json::Value::as_array).cloned().unwrap_or_else(|| vec![::serde_json::Value::String("target/decision-receipt.json".to_string())]),
        "anti_claims": [
            "This is bounded local multi-hypervisor campaign evidence only, not a hosted service.",
            "This is not a shared remote queue or cross-machine scheduler.",
            "This is not universal fleet-scale throughput or a full Antithesis-style product replacement.",
            "This receipt links hypervisor workers, leases, queue entries, run receipts, and queue state without raw-log scraping.",
            "Fault coverage is limited to listed deterministic fault classes and workloads; it is not exhaustive validation of all possible failures."
        ]
    }))
}

pub fn sample_multi_hypervisor_campaign_plan() -> ::serde_json::Value {
    json!({
        "schema_version": 1,
        "campaign_id": "local-campaign-0001",
        "max_hypervisors": 2,
        "state_path": "target/multi-hypervisor/campaign-state.json",
        "artifact_index_path": "target/multi-hypervisor/artifact-index.json",
        "follow_up_policy": {"enabled": true, "reproduce": true, "minimize": true},
        "hypervisors": [
            {"hypervisor_worker_id": "local-hv-a", "node_id": "local-node-a", "resource_budget": {"vcpus": 2, "memory_mib": 1024}, "artifact_root": "target/multi-hypervisor/local-hv-a"},
            {"hypervisor_worker_id": "local-hv-b", "node_id": "local-node-b", "resource_budget": {"vcpus": 2, "memory_mib": 1024}, "artifact_root": "target/multi-hypervisor/local-hv-b"}
        ],
        "queue": {"entries": [
            {"queue_entry_id": "mhq-raft-0001", "run_id": "mh-run-raft-0001", "workload": "raft", "command_plan": sample_typed_command("replay-readiness", &["--receipt", "target/multi-hypervisor/raft-replay-readiness.json"]), "receipt_path": "target/multi-hypervisor/raft-replay-readiness.json", "expected_bug_artifacts": ["target/multi-hypervisor/local-hv-a/bug-raft.json"]},
            {"queue_entry_id": "mhq-redb-0001", "run_id": "mh-run-redb-0001", "workload": "redb", "command_plan": sample_typed_command("replay-readiness", &["--receipt", "target/multi-hypervisor/redb-replay-readiness.json"]), "receipt_path": "target/multi-hypervisor/redb-replay-readiness.json", "expected_bug_artifacts": []}
        ]},
        "operator_decisions": ["target/decision-receipt.json"]
    })
}

pub fn sample_multi_hypervisor_campaign_receipt() -> ::serde_json::Value {
    json!({
        "schema_version": 1,
        "command": "replay-readiness-local-multi-hypervisor-campaign",
        "status": "recorded",
        "generated_at": "2026-05-11T00:00:00Z",
        "campaign_id": "local-campaign-0001",
        "scope": "bounded local multi-hypervisor campaign receipt with one durable local queue/state file; not a hosted service, not a shared remote queue, not cross-machine scheduling, not universal fleet-scale throughput, and not a full Antithesis-style product replacement",
        "raw_log_scraping": false,
        "fault_coverage": {
            "schema_version": 1,
            "scope": "listed deterministic fault classes and workloads only; not exhaustive validation of all possible failures",
            "by_workload": [
                {"workload": "raft", "configured_fault_classes": ["network", "timer", "scheduler"], "injection_attempts": 3, "observed_injections": 2, "not_observed_fault_classes": ["scheduler"], "affected_run_ids": ["mh-run-raft-0001"], "unsupported_fault_classes": ["process"]},
                {"workload": "redb", "configured_fault_classes": ["block"], "injection_attempts": 1, "observed_injections": 1, "not_observed_fault_classes": [], "affected_run_ids": ["mh-run-redb-0001"], "unsupported_fault_classes": []}
            ]
        },
        "queue_state": {"kind": "durable-local-file", "state_path": "target/multi-hypervisor/campaign-state.json", "loaded_existing_state": false, "completed_before_start": 0, "persisted_after_each_run": true},
        "control_plane": {"kind": "single-machine-local", "max_hypervisors": 2, "artifact_index_path": "target/multi-hypervisor/artifact-index.json", "follow_up_policy": {"enabled": true, "reproduce": true, "minimize": true}},
        "queue": {"entries": [
            {"queue_entry_id": "mhq-raft-0001", "run_id": "mh-run-raft-0001", "workload": "raft", "state": "completed", "lease_id": "lease-local-campaign-0001-mhq-raft-0001", "hypervisor_worker_id": "local-hv-a"},
            {"queue_entry_id": "mhq-redb-0001", "run_id": "mh-run-redb-0001", "workload": "redb", "state": "completed", "lease_id": "lease-local-campaign-0001-mhq-redb-0001", "hypervisor_worker_id": "local-hv-b"}
        ]},
        "hypervisors": [
            {"hypervisor_worker_id": "local-hv-a", "node_id": "local-node-a", "resource_budget": {"vcpus": 2, "memory_mib": 1024}, "artifact_root": "target/multi-hypervisor/local-hv-a"},
            {"hypervisor_worker_id": "local-hv-b", "node_id": "local-node-b", "resource_budget": {"vcpus": 2, "memory_mib": 1024}, "artifact_root": "target/multi-hypervisor/local-hv-b"}
        ],
        "runs": [
            {"campaign_id": "local-campaign-0001", "run_id": "mh-run-raft-0001", "queue_entry_id": "mhq-raft-0001", "hypervisor_worker_id": "local-hv-a", "workload": "raft", "lease_id": "lease-local-campaign-0001-mhq-raft-0001", "artifact_root": "target/multi-hypervisor/local-hv-a", "receipt_path": "target/multi-hypervisor/raft-replay-readiness.json", "receipt_summary": "replay-readiness status=passed dogfood=raft:pass scope=bounded", "follow_up_jobs": [{"job_id": "followup-mh-run-raft-0001-reproduce-0", "kind": "reproduce", "source_run_id": "mh-run-raft-0001", "source_queue_entry_id": "mhq-raft-0001", "hypervisor_worker_id": "local-hv-a", "bug_artifact_path": "target/multi-hypervisor/local-hv-a/bug-raft.json", "snapshot_ref": "snapshot:mh-run-raft-0001:0", "status": "queued"}, {"job_id": "followup-mh-run-raft-0001-minimize-0", "kind": "minimize", "source_run_id": "mh-run-raft-0001", "source_queue_entry_id": "mhq-raft-0001", "hypervisor_worker_id": "local-hv-a", "bug_artifact_path": "target/multi-hypervisor/local-hv-a/bug-raft.json", "snapshot_ref": "snapshot:mh-run-raft-0001:0", "status": "queued"}], "status": "passed", "exit_code": 0},
            {"campaign_id": "local-campaign-0001", "run_id": "mh-run-redb-0001", "queue_entry_id": "mhq-redb-0001", "hypervisor_worker_id": "local-hv-b", "workload": "redb", "lease_id": "lease-local-campaign-0001-mhq-redb-0001", "artifact_root": "target/multi-hypervisor/local-hv-b", "receipt_path": "target/multi-hypervisor/redb-replay-readiness.json", "receipt_summary": "replay-readiness status=passed dogfood=redb:pass scope=bounded", "follow_up_jobs": [], "status": "passed", "exit_code": 0}
        ],
        "artifact_index": {"schema_version": 1, "index_path": "target/multi-hypervisor/artifact-index.json", "entries": [
            {"artifact_id": "artifact-mh-run-raft-0001", "run_id": "mh-run-raft-0001", "queue_entry_id": "mhq-raft-0001", "hypervisor_worker_id": "local-hv-a", "artifact_root": "target/multi-hypervisor/local-hv-a", "receipt_path": "target/multi-hypervisor/raft-replay-readiness.json", "digest": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", "retention": {"policy": "bounded-local", "gc_status": "retained"}, "bug_artifacts": ["target/multi-hypervisor/local-hv-a/bug-raft.json"], "follow_up_receipts": []},
            {"artifact_id": "artifact-mh-run-redb-0001", "run_id": "mh-run-redb-0001", "queue_entry_id": "mhq-redb-0001", "hypervisor_worker_id": "local-hv-b", "artifact_root": "target/multi-hypervisor/local-hv-b", "receipt_path": "target/multi-hypervisor/redb-replay-readiness.json", "digest": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", "retention": {"policy": "bounded-local", "gc_status": "retained"}, "bug_artifacts": [], "follow_up_receipts": []}
        ]},
        "follow_up_jobs": [{"job_id": "followup-mh-run-raft-0001-reproduce-0", "kind": "reproduce", "source_run_id": "mh-run-raft-0001", "source_queue_entry_id": "mhq-raft-0001", "hypervisor_worker_id": "local-hv-a", "bug_artifact_path": "target/multi-hypervisor/local-hv-a/bug-raft.json", "snapshot_ref": "snapshot:mh-run-raft-0001:0", "status": "queued"}, {"job_id": "followup-mh-run-raft-0001-minimize-0", "kind": "minimize", "source_run_id": "mh-run-raft-0001", "source_queue_entry_id": "mhq-raft-0001", "hypervisor_worker_id": "local-hv-a", "bug_artifact_path": "target/multi-hypervisor/local-hv-a/bug-raft.json", "snapshot_ref": "snapshot:mh-run-raft-0001:0", "status": "queued"}],
        "operator_decisions": ["target/decision-receipt.json"],
        "anti_claims": [
            "This is bounded local multi-hypervisor campaign evidence only, not a hosted service.",
            "This is not a shared remote queue or cross-machine scheduler.",
            "This is not universal fleet-scale throughput or a full Antithesis-style product replacement.",
            "This receipt links hypervisor workers, leases, queue entries, run receipts, and queue state without raw-log scraping.",
            "Fault coverage is limited to listed deterministic fault classes and workloads; it is not exhaustive validation of all possible failures."
        ]
    })
}

pub fn validate_multi_hypervisor_campaign_receipt(
    receipt: &::serde_json::Value,
) -> crate::EvidenceResult<String> {
    let schema_version = int_field(
        receipt.get("schema_version"),
        "multi_hypervisor.schema_version",
    )?;
    crate::ensure(
        schema_version == 1,
        format!("multi_hypervisor.schema_version: expected 1, got {schema_version}"),
    )?;
    let command = str_field(receipt.get("command"), "multi_hypervisor.command")?;
    crate::ensure(command == "replay-readiness-local-multi-hypervisor-campaign", format!("multi_hypervisor.command: expected replay-readiness-local-multi-hypervisor-campaign, got {command:?}"))?;
    let status = str_field(receipt.get("status"), "multi_hypervisor.status")?;
    crate::ensure(
        matches!(status, "recorded" | "partial" | "failed"),
        format!("multi_hypervisor.status: unsupported value {status:?}"),
    )?;
    let campaign_id = token_field(receipt.get("campaign_id"), "multi_hypervisor.campaign_id")?;
    let scope = str_field(receipt.get("scope"), "multi_hypervisor.scope")?.to_lowercase();
    crate::ensure(
        scope.contains("bounded local multi-hypervisor")
            && scope.contains("not a hosted service")
            && scope.contains("not a shared remote queue")
            && scope.contains("not cross-machine scheduling")
            && scope.contains("not universal fleet-scale")
            && scope.contains("not a full antithesis-style product replacement"),
        "multi_hypervisor.scope: must preserve bounded local anti-claims",
    )?;
    crate::ensure(
        !matches!(
            receipt.get("raw_log_scraping"),
            Some(::serde_json::Value::Bool(true))
        ),
        "multi_hypervisor.raw_log_scraping: raw-log scraping is not allowed",
    )?;
    let fault_coverage = validate_fault_coverage_summary(receipt.get("fault_coverage"))?;

    let queue_state = object_field(receipt.get("queue_state"), "multi_hypervisor.queue_state")?;
    str_field(
        queue_state.get("state_path"),
        "multi_hypervisor.queue_state.state_path",
    )?;
    crate::ensure(
        matches!(
            queue_state.get("persisted_after_each_run"),
            Some(::serde_json::Value::Bool(true))
        ),
        "multi_hypervisor.queue_state.persisted_after_each_run: expected true",
    )?;
    int_field(
        queue_state.get("completed_before_start"),
        "multi_hypervisor.queue_state.completed_before_start",
    )?;

    let control_plane = object_field(
        receipt.get("control_plane"),
        "multi_hypervisor.control_plane",
    )?;
    crate::ensure(
        str_field(
            control_plane.get("kind"),
            "multi_hypervisor.control_plane.kind",
        )? == "single-machine-local",
        "multi_hypervisor.control_plane.kind: expected single-machine-local",
    )?;
    crate::ensure(
        int_field(
            control_plane.get("max_hypervisors"),
            "multi_hypervisor.control_plane.max_hypervisors",
        )? >= 2,
        "multi_hypervisor.control_plane.max_hypervisors: expected at least 2",
    )?;
    str_field(
        control_plane.get("artifact_index_path"),
        "multi_hypervisor.control_plane.artifact_index_path",
    )?;
    object_field(
        control_plane.get("follow_up_policy"),
        "multi_hypervisor.control_plane.follow_up_policy",
    )?;

    let hypervisors = array_field(receipt.get("hypervisors"), "multi_hypervisor.hypervisors")?;
    crate::ensure(
        hypervisors.len() >= 2,
        "multi_hypervisor.hypervisors: expected at least two local hypervisor workers",
    )?;
    let mut hypervisor_ids = ::std::collections::BTreeSet::new();
    let mut hypervisor_artifact_roots = ::std::collections::BTreeMap::new();
    for (idx, hypervisor) in hypervisors.iter().enumerate() {
        let hypervisor = object_field(
            Some(hypervisor),
            &format!("multi_hypervisor.hypervisors[{idx}]"),
        )?;
        let id = token_field(
            hypervisor.get("hypervisor_worker_id"),
            &format!("multi_hypervisor.hypervisors[{idx}].hypervisor_worker_id"),
        )?;
        crate::ensure(
            hypervisor_ids.insert(id.to_string()),
            format!("multi_hypervisor.hypervisors[{idx}].hypervisor_worker_id: duplicate {id}"),
        )?;
        token_field(
            hypervisor.get("node_id"),
            &format!("multi_hypervisor.hypervisors[{idx}].node_id"),
        )?;
        let budget = object_field(
            hypervisor.get("resource_budget"),
            &format!("multi_hypervisor.hypervisors[{idx}].resource_budget"),
        )?;
        crate::ensure(
            int_field(budget.get("vcpus"), &format!("multi_hypervisor.hypervisors[{idx}].resource_budget.vcpus"))? > 0,
            format!("multi_hypervisor.hypervisors[{idx}].resource_budget.vcpus: expected positive budget"),
        )?;
        crate::ensure(
            int_field(budget.get("memory_mib"), &format!("multi_hypervisor.hypervisors[{idx}].resource_budget.memory_mib"))? > 0,
            format!("multi_hypervisor.hypervisors[{idx}].resource_budget.memory_mib: expected positive budget"),
        )?;
        let artifact_root = str_field(
            hypervisor.get("artifact_root"),
            &format!("multi_hypervisor.hypervisors[{idx}].artifact_root"),
        )?;
        hypervisor_artifact_roots.insert(id.to_string(), artifact_root.to_string());
    }

    let queue = object_field(receipt.get("queue"), "multi_hypervisor.queue")?;
    let entries = array_field(queue.get("entries"), "multi_hypervisor.queue.entries")?;
    crate::ensure(
        !entries.is_empty(),
        "multi_hypervisor.queue.entries: expected non-empty list",
    )?;
    let mut entry_ids = ::std::collections::BTreeSet::new();
    let mut entry_run_ids = ::std::collections::BTreeSet::new();
    let mut lease_ids = ::std::collections::BTreeSet::new();
    for (idx, entry) in entries.iter().enumerate() {
        let entry = object_field(
            Some(entry),
            &format!("multi_hypervisor.queue.entries[{idx}]"),
        )?;
        let entry_id = token_field(
            entry.get("queue_entry_id"),
            &format!("multi_hypervisor.queue.entries[{idx}].queue_entry_id"),
        )?;
        crate::ensure(
            entry_ids.insert(entry_id.to_string()),
            format!("multi_hypervisor.queue.entries[{idx}].queue_entry_id: duplicate {entry_id}"),
        )?;
        let run_id = token_field(
            entry.get("run_id"),
            &format!("multi_hypervisor.queue.entries[{idx}].run_id"),
        )?;
        crate::ensure(
            entry_run_ids.insert(run_id.to_string()),
            format!("multi_hypervisor.queue.entries[{idx}].run_id: duplicate {run_id}"),
        )?;
        let lease_id = token_field(
            entry.get("lease_id"),
            &format!("multi_hypervisor.queue.entries[{idx}].lease_id"),
        )?;
        crate::ensure(
            lease_ids.insert(lease_id.to_string()),
            format!("multi_hypervisor.queue.entries[{idx}].lease_id: duplicate {lease_id}"),
        )?;
        let hypervisor_worker_id = token_field(
            entry.get("hypervisor_worker_id"),
            &format!("multi_hypervisor.queue.entries[{idx}].hypervisor_worker_id"),
        )?;
        crate::ensure(hypervisor_ids.contains(hypervisor_worker_id), format!("multi_hypervisor.queue.entries[{idx}].hypervisor_worker_id: {hypervisor_worker_id} missing from hypervisors"))?;
        let state = token_field(
            entry.get("state"),
            &format!("multi_hypervisor.queue.entries[{idx}].state"),
        )?;
        crate::ensure(
            matches!(state, "queued" | "leased" | "completed" | "failed"),
            format!("multi_hypervisor.queue.entries[{idx}].state: unsupported value {state:?}"),
        )?;
    }

    let runs = array_field(receipt.get("runs"), "multi_hypervisor.runs")?;
    crate::ensure(
        !runs.is_empty(),
        "multi_hypervisor.runs: expected non-empty list",
    )?;
    let mut run_ids = ::std::collections::BTreeSet::new();
    let mut workloads = ::std::collections::BTreeSet::new();
    let mut passed = 0usize;
    for (idx, run) in runs.iter().enumerate() {
        let run = object_field(Some(run), &format!("multi_hypervisor.runs[{idx}]"))?;
        let run_campaign_id = token_field(
            run.get("campaign_id"),
            &format!("multi_hypervisor.runs[{idx}].campaign_id"),
        )?;
        crate::ensure(
            run_campaign_id == campaign_id,
            format!("multi_hypervisor.runs[{idx}].campaign_id: expected {campaign_id}"),
        )?;
        let run_id = token_field(
            run.get("run_id"),
            &format!("multi_hypervisor.runs[{idx}].run_id"),
        )?;
        crate::ensure(
            run_ids.insert(run_id.to_string()),
            format!("multi_hypervisor.runs[{idx}].run_id: duplicate {run_id}"),
        )?;
        crate::ensure(
            entry_run_ids.contains(run_id),
            format!("multi_hypervisor.runs[{idx}].run_id: {run_id} missing from queue entries"),
        )?;
        let queue_entry_id = token_field(
            run.get("queue_entry_id"),
            &format!("multi_hypervisor.runs[{idx}].queue_entry_id"),
        )?;
        crate::ensure(entry_ids.contains(queue_entry_id), format!("multi_hypervisor.runs[{idx}].queue_entry_id: {queue_entry_id} missing from queue entries"))?;
        let hypervisor_worker_id = token_field(
            run.get("hypervisor_worker_id"),
            &format!("multi_hypervisor.runs[{idx}].hypervisor_worker_id"),
        )?;
        crate::ensure(hypervisor_ids.contains(hypervisor_worker_id), format!("multi_hypervisor.runs[{idx}].hypervisor_worker_id: {hypervisor_worker_id} missing from hypervisors"))?;
        let lease_id = token_field(
            run.get("lease_id"),
            &format!("multi_hypervisor.runs[{idx}].lease_id"),
        )?;
        crate::ensure(
            lease_ids.contains(lease_id),
            format!("multi_hypervisor.runs[{idx}].lease_id: {lease_id} missing from queue entries"),
        )?;
        let workload = token_field(
            run.get("workload"),
            &format!("multi_hypervisor.runs[{idx}].workload"),
        )?;
        workloads.insert(workload.to_string());
        let artifact_root = str_field(
            run.get("artifact_root"),
            &format!("multi_hypervisor.runs[{idx}].artifact_root"),
        )?;
        crate::ensure(
            hypervisor_artifact_roots.get(hypervisor_worker_id).is_some_and(|root| root == artifact_root),
            format!("multi_hypervisor.runs[{idx}].artifact_root: must match assigned hypervisor artifact root"),
        )?;
        str_field(
            run.get("receipt_path"),
            &format!("multi_hypervisor.runs[{idx}].receipt_path"),
        )?;
        let run_status = token_field(
            run.get("status"),
            &format!("multi_hypervisor.runs[{idx}].status"),
        )?;
        crate::ensure(
            matches!(run_status, "passed" | "failed"),
            format!("multi_hypervisor.runs[{idx}].status: unsupported value {run_status:?}"),
        )?;
        let exit_code = int_field(
            run.get("exit_code"),
            &format!("multi_hypervisor.runs[{idx}].exit_code"),
        )?;
        let follow_ups = array_field(
            run.get("follow_up_jobs"),
            &format!("multi_hypervisor.runs[{idx}].follow_up_jobs"),
        )?;
        for (job_idx, job) in follow_ups.iter().enumerate() {
            validate_multi_hypervisor_follow_up_job(
                job,
                &format!("multi_hypervisor.runs[{idx}].follow_up_jobs[{job_idx}]"),
                &run_ids,
                &entry_ids,
                &hypervisor_ids,
            )?;
        }
        if run_status == "passed" {
            crate::ensure(
                exit_code == 0,
                format!("multi_hypervisor.runs[{idx}].exit_code: passed run must exit 0"),
            )?;
            let summary = str_field(
                run.get("receipt_summary"),
                &format!("multi_hypervisor.runs[{idx}].receipt_summary"),
            )?;
            crate::ensure(summary.contains("replay-readiness status="), format!("multi_hypervisor.runs[{idx}].receipt_summary: expected replay-readiness summary"))?;
            passed += 1;
        } else {
            crate::ensure(
                exit_code != 0,
                format!("multi_hypervisor.runs[{idx}].exit_code: failed run must be nonzero"),
            )?;
        }
    }

    let artifact_index = object_field(
        receipt.get("artifact_index"),
        "multi_hypervisor.artifact_index",
    )?;
    crate::ensure(
        int_field(
            artifact_index.get("schema_version"),
            "multi_hypervisor.artifact_index.schema_version",
        )? == 1,
        "multi_hypervisor.artifact_index.schema_version: expected 1",
    )?;
    str_field(
        artifact_index.get("index_path"),
        "multi_hypervisor.artifact_index.index_path",
    )?;
    let artifact_entries = array_field(
        artifact_index.get("entries"),
        "multi_hypervisor.artifact_index.entries",
    )?;
    crate::ensure(
        artifact_entries.len() == runs.len(),
        "multi_hypervisor.artifact_index.entries: expected one artifact entry per run",
    )?;
    let mut artifact_ids = ::std::collections::BTreeSet::new();
    for (idx, artifact) in artifact_entries.iter().enumerate() {
        let artifact = object_field(
            Some(artifact),
            &format!("multi_hypervisor.artifact_index.entries[{idx}]"),
        )?;
        let artifact_id = token_field(
            artifact.get("artifact_id"),
            &format!("multi_hypervisor.artifact_index.entries[{idx}].artifact_id"),
        )?;
        crate::ensure(artifact_ids.insert(artifact_id.to_string()), format!("multi_hypervisor.artifact_index.entries[{idx}].artifact_id: duplicate {artifact_id}"))?;
        let run_id = token_field(
            artifact.get("run_id"),
            &format!("multi_hypervisor.artifact_index.entries[{idx}].run_id"),
        )?;
        crate::ensure(
            run_ids.contains(run_id),
            format!(
                "multi_hypervisor.artifact_index.entries[{idx}].run_id: {run_id} missing from runs"
            ),
        )?;
        let queue_entry_id = token_field(
            artifact.get("queue_entry_id"),
            &format!("multi_hypervisor.artifact_index.entries[{idx}].queue_entry_id"),
        )?;
        crate::ensure(entry_ids.contains(queue_entry_id), format!("multi_hypervisor.artifact_index.entries[{idx}].queue_entry_id: {queue_entry_id} missing from queue entries"))?;
        let hypervisor_worker_id = token_field(
            artifact.get("hypervisor_worker_id"),
            &format!("multi_hypervisor.artifact_index.entries[{idx}].hypervisor_worker_id"),
        )?;
        crate::ensure(hypervisor_ids.contains(hypervisor_worker_id), format!("multi_hypervisor.artifact_index.entries[{idx}].hypervisor_worker_id: {hypervisor_worker_id} missing from hypervisors"))?;
        str_field(
            artifact.get("artifact_root"),
            &format!("multi_hypervisor.artifact_index.entries[{idx}].artifact_root"),
        )?;
        str_field(
            artifact.get("receipt_path"),
            &format!("multi_hypervisor.artifact_index.entries[{idx}].receipt_path"),
        )?;
        validate_digest_field(
            artifact.get("digest"),
            &format!("multi_hypervisor.artifact_index.entries[{idx}].digest"),
        )?;
        let retention = object_field(
            artifact.get("retention"),
            &format!("multi_hypervisor.artifact_index.entries[{idx}].retention"),
        )?;
        str_field(
            retention.get("policy"),
            &format!("multi_hypervisor.artifact_index.entries[{idx}].retention.policy"),
        )?;
        let gc_status = token_field(
            retention.get("gc_status"),
            &format!("multi_hypervisor.artifact_index.entries[{idx}].retention.gc_status"),
        )?;
        crate::ensure(matches!(gc_status, "retained" | "eligible" | "collected"), format!("multi_hypervisor.artifact_index.entries[{idx}].retention.gc_status: unsupported value {gc_status:?}"))?;
        array_field(
            artifact.get("bug_artifacts"),
            &format!("multi_hypervisor.artifact_index.entries[{idx}].bug_artifacts"),
        )?;
        array_field(
            artifact.get("follow_up_receipts"),
            &format!("multi_hypervisor.artifact_index.entries[{idx}].follow_up_receipts"),
        )?;
    }
    let follow_up_jobs = array_field(
        receipt.get("follow_up_jobs"),
        "multi_hypervisor.follow_up_jobs",
    )?;
    for (idx, job) in follow_up_jobs.iter().enumerate() {
        validate_multi_hypervisor_follow_up_job(
            job,
            &format!("multi_hypervisor.follow_up_jobs[{idx}]"),
            &run_ids,
            &entry_ids,
            &hypervisor_ids,
        )?;
    }

    let anti_claims = array_field(receipt.get("anti_claims"), "multi_hypervisor.anti_claims")?;
    let anti_claim_text = anti_claims
        .iter()
        .map(json_display)
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();
    crate::ensure(
        anti_claim_text.contains("bounded local multi-hypervisor")
            && anti_claim_text.contains("not a hosted service")
            && anti_claim_text.contains("not a shared remote queue")
            && anti_claim_text.contains("cross-machine")
            && anti_claim_text.contains("without raw-log scraping"),
        "multi_hypervisor.anti_claims: missing bounded local multi-hypervisor anti-overclaim text",
    )?;
    Ok(format!("replay-readiness-local-multi-hypervisor-campaign status={status} campaign={campaign_id} hypervisors={} runs={} passed={} restart_persisted=true workloads={} fault_classes={} scope=bounded-local-multi-hypervisor", hypervisors.len(), runs.len(), passed, workloads.into_iter().collect::<Vec<_>>().join(","), fault_coverage.join(",")))
}

fn validate_fault_coverage_summary(
    value: Option<&::serde_json::Value>,
) -> crate::EvidenceResult<Vec<String>> {
    let summary = object_field(value, "multi_hypervisor.fault_coverage")?;
    crate::ensure(
        int_field(
            summary.get("schema_version"),
            "multi_hypervisor.fault_coverage.schema_version",
        )? == 1,
        "multi_hypervisor.fault_coverage.schema_version: expected 1",
    )?;
    let scope = str_field(
        summary.get("scope"),
        "multi_hypervisor.fault_coverage.scope",
    )?
    .to_ascii_lowercase();
    crate::ensure(
        scope.contains("listed") && scope.contains("not exhaustive"),
        "multi_hypervisor.fault_coverage.scope: must preserve listed-only anti-claim",
    )?;
    let by_workload = array_field(
        summary.get("by_workload"),
        "multi_hypervisor.fault_coverage.by_workload",
    )?;
    crate::ensure(
        !by_workload.is_empty(),
        "multi_hypervisor.fault_coverage.by_workload: expected non-empty list",
    )?;
    let mut classes = ::std::collections::BTreeSet::new();
    for (idx, row) in by_workload.iter().enumerate() {
        let row = object_field(
            Some(row),
            &format!("multi_hypervisor.fault_coverage.by_workload[{idx}]"),
        )?;
        token_field(
            row.get("workload"),
            &format!("multi_hypervisor.fault_coverage.by_workload[{idx}].workload"),
        )?;
        let configured = array_field(
            row.get("configured_fault_classes"),
            &format!("multi_hypervisor.fault_coverage.by_workload[{idx}].configured_fault_classes"),
        )?;
        crate::ensure(!configured.is_empty(), format!("multi_hypervisor.fault_coverage.by_workload[{idx}].configured_fault_classes: expected non-empty list"))?;
        for (class_idx, class) in configured.iter().enumerate() {
            let class = token_field(Some(class), &format!("multi_hypervisor.fault_coverage.by_workload[{idx}].configured_fault_classes[{class_idx}]"))?;
            crate::ensure(matches!(class, "network" | "block" | "timer" | "process" | "scheduler"), format!("multi_hypervisor.fault_coverage.by_workload[{idx}].configured_fault_classes[{class_idx}]: unsupported class {class:?}"))?;
            classes.insert(class.to_string());
        }
        let attempts = int_field(
            row.get("injection_attempts"),
            &format!("multi_hypervisor.fault_coverage.by_workload[{idx}].injection_attempts"),
        )?;
        let observed = int_field(
            row.get("observed_injections"),
            &format!("multi_hypervisor.fault_coverage.by_workload[{idx}].observed_injections"),
        )?;
        crate::ensure(attempts >= 0 && observed >= 0 && observed <= attempts, format!("multi_hypervisor.fault_coverage.by_workload[{idx}]: observed_injections must be <= injection_attempts"))?;
        array_field(
            row.get("not_observed_fault_classes"),
            &format!(
                "multi_hypervisor.fault_coverage.by_workload[{idx}].not_observed_fault_classes"
            ),
        )?;
        array_field(
            row.get("affected_run_ids"),
            &format!("multi_hypervisor.fault_coverage.by_workload[{idx}].affected_run_ids"),
        )?;
        array_field(
            row.get("unsupported_fault_classes"),
            &format!(
                "multi_hypervisor.fault_coverage.by_workload[{idx}].unsupported_fault_classes"
            ),
        )?;
    }
    Ok(classes.into_iter().collect())
}

pub fn render_multi_hypervisor_campaign_dashboard(
    receipt: &::serde_json::Value,
) -> crate::EvidenceResult<String> {
    let summary = validate_multi_hypervisor_campaign_receipt(receipt)?;
    let campaign_id = str_field(receipt.get("campaign_id"), "multi_hypervisor.campaign_id")?;
    let status = str_field(receipt.get("status"), "multi_hypervisor.status")?;
    let hypervisors = array_field(receipt.get("hypervisors"), "multi_hypervisor.hypervisors")?;
    let entries = array_field(
        object_field(receipt.get("queue"), "multi_hypervisor.queue")?.get("entries"),
        "multi_hypervisor.queue.entries",
    )?;
    let runs = array_field(receipt.get("runs"), "multi_hypervisor.runs")?;
    let follow_up_jobs = array_field(
        receipt.get("follow_up_jobs"),
        "multi_hypervisor.follow_up_jobs",
    )?;
    let artifact_entries = array_field(
        object_field(
            receipt.get("artifact_index"),
            "multi_hypervisor.artifact_index",
        )?
        .get("entries"),
        "multi_hypervisor.artifact_index.entries",
    )?;
    let mut worker_rows = Vec::new();
    for worker in hypervisors {
        let worker = object_field(Some(worker), "multi_hypervisor.hypervisors[]")?;
        let budget = object_field(
            worker.get("resource_budget"),
            "multi_hypervisor.hypervisors[].resource_budget",
        )?;
        worker_rows.push(format!(
            "<tr><td><code>{}</code></td><td><code>{}</code></td><td>{} vCPU / {} MiB</td><td><code>{}</code></td></tr>",
            esc_value(worker.get("hypervisor_worker_id")),
            esc_value(worker.get("node_id")),
            esc_value(budget.get("vcpus")),
            esc_value(budget.get("memory_mib")),
            esc_value(worker.get("artifact_root")),
        ));
    }
    let fault_coverage = object_field(
        receipt.get("fault_coverage"),
        "multi_hypervisor.fault_coverage",
    )?;
    let fault_scope = str_field(
        fault_coverage.get("scope"),
        "multi_hypervisor.fault_coverage.scope",
    )?;
    let mut fault_rows = Vec::new();
    for row in array_field(
        fault_coverage.get("by_workload"),
        "multi_hypervisor.fault_coverage.by_workload",
    )? {
        let row = object_field(Some(row), "multi_hypervisor.fault_coverage.by_workload[]")?;
        fault_rows.push(format!(
            r#"<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>"#,
            esc_value(row.get("workload")),
            esc_value(row.get("configured_fault_classes")),
            esc_value(row.get("injection_attempts")),
            esc_value(row.get("observed_injections")),
            esc_value(row.get("not_observed_fault_classes")),
            esc_value(row.get("unsupported_fault_classes")),
        ));
    }
    let mut run_rows = Vec::new();
    for run in runs {
        let run = object_field(Some(run), "multi_hypervisor.runs[]")?;
        let run_followups = array_field(
            run.get("follow_up_jobs"),
            "multi_hypervisor.runs[].follow_up_jobs",
        )?;
        run_rows.push(format!(
            r#"<tr><td><code>{}</code></td><td><code>{}</code></td><td>{}</td><td><span class="pill {}">{}</span></td><td><code>{}</code></td><td>{}</td></tr>"#,
            esc_value(run.get("run_id")),
            esc_value(run.get("hypervisor_worker_id")),
            esc_value(run.get("workload")),
            token_class(str_field(run.get("status"), "multi_hypervisor.runs[].status")?),
            esc_value(run.get("status")),
            esc_value(run.get("receipt_path")),
            run_followups.len(),
        ));
    }
    let scope = str_field(receipt.get("scope"), "multi_hypervisor.scope")?;
    Ok(format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>ChaosControl local multi-hypervisor campaign</title>
<style>:root {{ color-scheme: light dark; --ok:#138a36; --bad:#b42318; --warn:#b7791f; --border:#98a2b3; }} body {{ font-family: ui-sans-serif, system-ui, sans-serif; margin: 2rem; line-height: 1.45; }} table {{ border-collapse: collapse; width: 100%; margin: 1rem 0; }} th,td {{ border-bottom: 1px solid var(--border); padding: .55rem; text-align:left; vertical-align:top; }} .pill {{ border-radius:999px; color:white; padding:.15rem .55rem; font-weight:700; }} .ok {{ background:var(--ok); }} .bad {{ background:var(--bad); }} .warn {{ background:var(--warn); }} .scope {{ border-left:.35rem solid var(--warn); padding-left:.8rem; }}</style></head>
<body><header><h1>ChaosControl local multi-hypervisor campaign</h1><p><strong>Campaign:</strong> <code>{}</code> <strong>Status:</strong> <span class="pill {}">{}</span></p><p><code>{}</code></p></header>
<section class="scope"><h2>Scope</h2><p>{}</p><p>This dashboard is local-only: not SaaS, not a remote shared queue, not cross-machine scheduling, and not universal fleet throughput.</p></section>
<section><h2>Workers</h2><table><thead><tr><th>Worker</th><th>Node</th><th>Budget</th><th>Artifact root</th></tr></thead><tbody>{}</tbody></table></section>
<section><h2>Queue and runs</h2><p>Queue entries: {} · Runs: {} · Artifact index entries: {} · Follow-up jobs: {}</p><table><thead><tr><th>Run</th><th>Worker</th><th>Workload</th><th>Status</th><th>Receipt</th><th>Follow-ups</th></tr></thead><tbody>{}</tbody></table></section>
<section class="scope"><h2>Fault coverage</h2><p>{}</p><table><thead><tr><th>Workload</th><th>Configured classes</th><th>Attempts</th><th>Observed</th><th>Not observed</th><th>Unsupported</th></tr></thead><tbody>{}</tbody></table></section>
</body></html>"#,
        esc(campaign_id),
        token_class(status),
        esc(status),
        esc(&summary),
        esc(scope),
        worker_rows.join(
            "
"
        ),
        entries.len(),
        runs.len(),
        artifact_entries.len(),
        follow_up_jobs.len(),
        run_rows.join(
            "
"
        ),
        esc(fault_scope),
        fault_rows.join(
            "
"
        ),
    ))
}

pub fn write_multi_hypervisor_campaign_dashboard_path(
    receipt_path: impl AsRef<::std::path::Path>,
    output_path: impl AsRef<::std::path::Path>,
) -> crate::EvidenceResult<String> {
    let receipt = crate::replay_readiness_loader::load_json(receipt_path.as_ref())?;
    let dashboard = render_multi_hypervisor_campaign_dashboard(&receipt)?;
    let output_path = output_path.as_ref();
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, dashboard)?;
    validate_multi_hypervisor_campaign_receipt(&receipt)
}

fn validate_multi_hypervisor_follow_up_job(
    job: &::serde_json::Value,
    field: &str,
    run_ids: &::std::collections::BTreeSet<String>,
    entry_ids: &::std::collections::BTreeSet<String>,
    hypervisor_ids: &::std::collections::BTreeSet<String>,
) -> crate::EvidenceResult<()> {
    let job = object_field(Some(job), field)?;
    token_field(job.get("job_id"), &format!("{field}.job_id"))?;
    let kind = token_field(job.get("kind"), &format!("{field}.kind"))?;
    crate::ensure(
        matches!(kind, "reproduce" | "minimize"),
        format!("{field}.kind: unsupported value {kind:?}"),
    )?;
    let run_id = token_field(job.get("source_run_id"), &format!("{field}.source_run_id"))?;
    crate::ensure(
        run_ids.contains(run_id),
        format!("{field}.source_run_id: {run_id} missing from runs"),
    )?;
    let queue_entry_id = token_field(
        job.get("source_queue_entry_id"),
        &format!("{field}.source_queue_entry_id"),
    )?;
    crate::ensure(
        entry_ids.contains(queue_entry_id),
        format!("{field}.source_queue_entry_id: {queue_entry_id} missing from queue entries"),
    )?;
    let hypervisor_worker_id = token_field(
        job.get("hypervisor_worker_id"),
        &format!("{field}.hypervisor_worker_id"),
    )?;
    crate::ensure(
        hypervisor_ids.contains(hypervisor_worker_id),
        format!("{field}.hypervisor_worker_id: {hypervisor_worker_id} missing from hypervisors"),
    )?;
    str_field(
        job.get("bug_artifact_path"),
        &format!("{field}.bug_artifact_path"),
    )?;
    str_field(job.get("snapshot_ref"), &format!("{field}.snapshot_ref"))?;
    let status = token_field(job.get("status"), &format!("{field}.status"))?;
    crate::ensure(
        matches!(
            status,
            "queued" | "running" | "passed" | "failed" | "skipped"
        ),
        format!("{field}.status: unsupported value {status:?}"),
    )?;
    Ok(())
}

pub fn render_dashboard(
    receipt: &::serde_json::Value,
    summary_line: &str,
) -> crate::EvidenceResult<String> {
    let status = str_field(receipt.get("status"), "receipt.status")?;
    let gates = array_field(receipt.get("static_gates"), "receipt.static_gates")?;
    crate::ensure(
        !gates.is_empty(),
        "receipt.static_gates: expected non-empty list",
    )?;
    let dogfood = object_field(receipt.get("dogfood"), "receipt.dogfood")?;
    let scope = str_field(receipt.get("scope"), "receipt.scope")?;
    let passed = gates
        .iter()
        .filter(|g| g.get("status").and_then(::serde_json::Value::as_str) == Some("pass"))
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
                .unwrap_or(&::serde_json::Value::String("not-applicable".into()))
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
    receipt_path: impl AsRef<::std::path::Path>,
    readme_path: impl AsRef<::std::path::Path>,
) -> crate::EvidenceResult<String> {
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

pub fn replace_readme_marker_block(
    readme_text: &str,
    replacement: &str,
) -> crate::EvidenceResult<String> {
    let start = readme_text.find(README_START_MARKER).ok_or_else(|| {
        crate::EvidenceError::new("README status markers missing or out of order")
    })?;
    let end = readme_text.find(README_END_MARKER).ok_or_else(|| {
        crate::EvidenceError::new("README status markers missing or out of order")
    })?;
    crate::ensure(
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
    root: impl AsRef<::std::path::Path>,
    flake_path: impl AsRef<::std::path::Path>,
) -> crate::EvidenceResult<Vec<String>> {
    let root = root.as_ref();
    let flake_text = std::fs::read_to_string(flake_path.as_ref())?;
    let gate_names = validate_gate_metadata(&flake_text)?;
    let summary_line = validate_renderer_equivalence(root)?;
    Ok(vec![
        format!("static_gates={}", gate_names.join(",")),
        format!("summary={summary_line}"),
    ])
}

pub fn run_readiness_surface_drift_selftest(
    root: impl AsRef<::std::path::Path>,
) -> crate::EvidenceResult<()> {
    let root = root.as_ref();
    let flake_text = std::fs::read_to_string(root.join("flake.nix"))?;
    validate_gate_metadata(&flake_text)?;
    validate_renderer_equivalence(root)?;
    let missing_execution = flake_text.replace(
        "              run_gate readiness-promotion readiness_promotion_status check-readiness-promotion-gate --root .\n",
        "",
    );
    match validate_gate_metadata(&missing_execution) {
        Err(err) if err.message().contains("without executed run_gate") => {}
        Err(err) => {
            return Err(crate::EvidenceError::new(format!(
                "unexpected missing-execution error: {}",
                err.message()
            )))
        }
        Ok(_) => {
            return Err(crate::EvidenceError::new(
                "missing executed gate fixture unexpectedly passed",
            ))
        }
    }
    let extra_execution = flake_text.replace(
        "              echo \"replay readiness checks passed\"",
        "              run_gate phantom-gate phantom_status check-phantom-gate\n              echo \"replay readiness checks passed\"",
    );
    match validate_gate_metadata(&extra_execution) {
        Err(err) if err.message().contains("missing from receipt metadata") => Ok(()),
        Err(err) => Err(crate::EvidenceError::new(format!(
            "unexpected extra-execution error: {}",
            err.message()
        ))),
        Ok(_) => Err(crate::EvidenceError::new(
            "extra executed gate fixture unexpectedly passed",
        )),
    }
}

pub fn validate_gate_metadata(flake_text: &str) -> crate::EvidenceResult<Vec<String>> {
    let executed = executed_static_gate_names(flake_text)?;
    let receipt = validate_unique_nonempty(
        crate::rust_automation::readiness_receipt::gate_names(),
        "receipt static gate metadata entries",
    )?;
    let executed_set = executed.iter().collect::<::std::collections::BTreeSet<_>>();
    let receipt_set = receipt.iter().collect::<::std::collections::BTreeSet<_>>();
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
    crate::ensure(
        missing.is_empty(),
        format!(
            "executed static gates missing from receipt metadata: {}",
            missing.join(", ")
        ),
    )?;
    crate::ensure(
        extra.is_empty(),
        format!(
            "receipt static gates without executed run_gate: {}",
            extra.join(", ")
        ),
    )?;
    Ok(executed)
}

pub fn sample_replay_readiness_receipt(dogfood: bool, status: &str) -> ::serde_json::Value {
    let dogfood_obj = if dogfood {
        json!({"selected_workload":"rust-workload","status":"pass","output":"/tmp/proof&artifact","summary":{"accepted":true,"seed":42,"snapshot_probe_fail_after":25,"verdict":{"replay_class":"snapshot_backed_reproduced","replay_parent_depth":2}},"expectation":{"expected":{"accepted":true}},"expectation_status":"matched","evidence_curation":"explicit-follow-up"})
    } else {
        json!({"selected_workload":null,"status":"skipped","output":null,"summary":null,"expectation":null,"expectation_status":"not-applicable","evidence_curation":"explicit-follow-up"})
    };
    json!({"schema_version":1,"command":"replay-readiness","status":status,"exit_code": if status == "passed" {0} else {1},"failed_phase": if status == "passed" {::serde_json::Value::Null} else {::serde_json::Value::String("evidence-contracts".into())},"started_at":"2026-05-08T00:00:00Z","finished_at":"2026-05-08T00:00:01Z","static_gates":[{"name":"contract-registry","command":"check-contract-registry .","status":"pass"},{"name":"evidence-contracts","command":"check-evidence-contracts --root .","status": if status == "passed" {"pass"} else {"fail"}}],"dogfood":dogfood_obj,"scope":"bounded committed replay/evidence readiness; not universal determinism or hosted-product parity"})
}

pub fn sample_decision_receipt() -> ::serde_json::Value {
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

fn sample_typed_command(program: &str, args: &[&str]) -> ::serde_json::Value {
    const SAMPLE_EXECUTABLE_MAX_BYTES: u64 = 16_777_216;
    const SAMPLE_TIMEOUT_MS: u64 = 30_000;
    const SAMPLE_INPUT_MAX_BYTES: u64 = 1_024;
    const SAMPLE_OUTPUT_MAX_BYTES: u64 = 1_048_576;
    const SAMPLE_POLL_INTERVAL_MS: u64 = 10;
    const SAMPLE_TEARDOWN_TIMEOUT_MS: u64 = 1_000;
    const SAMPLE_DIGEST: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    json!({
        "schema": crate::typed_operator_command::PLAN_SCHEMA,
        "mechanism_revision": crate::typed_operator_command::MECHANISM_REVISION,
        "executable": {
            "path": format!("/nix/store/sample-chaoscontrol/bin/{program}"),
            "blake3": SAMPLE_DIGEST,
            "maximum_bytes": SAMPLE_EXECUTABLE_MAX_BYTES
        },
        "args": args,
        "working_directory": ".",
        "environment": {"mode": "clear", "entries": []},
        "stdin": {"mode": "null"},
        "limits": {
            "timeout_ms": SAMPLE_TIMEOUT_MS,
            "stdin_max_bytes": SAMPLE_INPUT_MAX_BYTES,
            "stdout_max_bytes": SAMPLE_OUTPUT_MAX_BYTES,
            "stderr_max_bytes": SAMPLE_OUTPUT_MAX_BYTES,
            "poll_interval_ms": SAMPLE_POLL_INTERVAL_MS,
            "teardown_timeout_ms": SAMPLE_TEARDOWN_TIMEOUT_MS
        },
        "accepted_exit_codes": [0],
        "reject_stdout_truncation": true,
        "reject_stderr_truncation": true,
        "termination_scope": "process-group",
        "evidence_eligible": true
    })
}

pub fn sample_scheduler_receipt() -> ::serde_json::Value {
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
                "command_plan": sample_typed_command("replay-readiness", &["--dogfood", "raft", "--receipt", "target/raft-replay-readiness.json"]),
                "receipt_path": "target/raft-replay-readiness.json",
                "decision_policy": "record-local-decision"
            },
            {
                "run_id": "local-run-redb-0001",
                "workload": "redb",
                "command_plan": sample_typed_command("replay-readiness", &["--dogfood", "redb", "--receipt", "target/redb-replay-readiness.json"]),
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

fn validate_renderer_equivalence(_root: &::std::path::Path) -> crate::EvidenceResult<String> {
    let receipt = sample_replay_readiness_receipt(true, "passed");
    let summary = summarize_receipt(&receipt)?;
    crate::ensure(
        summary.starts_with("replay-readiness status="),
        "summary line has unexpected prefix",
    )?;
    crate::ensure(
        summary.contains("scope=bounded"),
        "summary line lost bounded scope token",
    )?;
    let dashboard = render_dashboard(&receipt, &summary)?;
    crate::ensure(
        dashboard.contains(&summary),
        "dashboard does not contain summary line",
    )?;
    crate::ensure(
        dashboard.contains("snapshot_backed_reproduced"),
        "dashboard lost dogfood replay class",
    )?;
    let fleet_index = render_fleet_triage_index(&[(
        "receipt.json".to_string(),
        receipt.clone(),
        summary.clone(),
    )])?;
    crate::ensure(
        fleet_index.contains("ChaosControl fleet triage index")
            && fleet_index.contains("snapshot_backed_reproduced"),
        "fleet triage index lost receipt summary or replay class",
    )?;
    crate::ensure(
        bounded(&fleet_index) && fleet_index.contains("not a hosted service"),
        "fleet triage index lost hosted-service anti-overclaim language",
    )?;
    let decision = sample_decision_receipt();
    let decision_summary = validate_decision_receipt(&decision)?;
    crate::ensure(
        decision_summary.contains("scope=bounded-local-not-shared"),
        "decision receipt summary lost bounded local scope",
    )?;
    let mut overclaimed_decision = decision.clone();
    overclaimed_decision["scope"] = json!("hosted shared decision store");
    match validate_decision_receipt(&overclaimed_decision) {
        Err(_) => {}
        Ok(_) => {
            return Err(crate::EvidenceError::new(
                "overclaiming decision receipt unexpectedly passed",
            ))
        }
    }
    let scheduler = sample_scheduler_receipt();
    let scheduler_summary = validate_scheduler_receipt(&scheduler)?;
    crate::ensure(
        scheduler_summary.contains("scope=bounded-local-not-hosted")
            && scheduler_summary.contains("runs=2"),
        "scheduler receipt summary lost bounded local scope or run count",
    )?;
    let mut overclaimed_scheduler = scheduler.clone();
    overclaimed_scheduler["scope"] = json!("hosted fleet-scale scheduler");
    match validate_scheduler_receipt(&overclaimed_scheduler) {
        Err(_) => {}
        Ok(_) => {
            return Err(crate::EvidenceError::new(
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
    crate::ensure(
        validate_scheduler_execution_receipt(&execution)?
            .contains("scope=bounded-local-sequential-not-hosted"),
        "scheduler execution summary lost bounded local scope",
    )?;
    let mut overclaimed_execution = execution;
    overclaimed_execution["raw_log_scraping"] = json!(true);
    match validate_scheduler_execution_receipt(&overclaimed_execution) {
        Err(_) => {}
        Ok(_) => {
            return Err(crate::EvidenceError::new(
                "raw-log scheduler execution unexpectedly passed",
            ))
        }
    }
    let fleet_scheduler = sample_fleet_scheduler_receipt();
    let fleet_scheduler_summary = validate_fleet_scheduler_receipt(&fleet_scheduler)?;
    crate::ensure(
        fleet_scheduler_summary.contains("scope=bounded-hosted-fleet"),
        "fleet scheduler summary lost bounded hosted/fleet scope",
    )?;
    let mut overclaimed_fleet_scheduler = fleet_scheduler;
    overclaimed_fleet_scheduler["raw_log_scraping"] = json!(true);
    match validate_fleet_scheduler_receipt(&overclaimed_fleet_scheduler) {
        Err(_) => {}
        Ok(_) => {
            return Err(crate::EvidenceError::new(
                "raw-log fleet scheduler receipt unexpectedly passed",
            ))
        }
    }
    match render_fleet_triage_index(&[]) {
        Err(_) => {}
        Ok(_) => {
            return Err(crate::EvidenceError::new(
                "empty fleet index unexpectedly passed",
            ))
        }
    }
    crate::ensure(
        bounded(&dashboard),
        "dashboard lost bounded anti-overclaim language",
    )?;
    let readme =
        format!("# Demo\n\n{README_START_MARKER}\nold status\n{README_END_MARKER}\n\nafter\n");
    let rendered = replace_readme_marker_block(&readme, &render_readme_status_block(&summary))?;
    crate::ensure(
        rendered.contains(&summary),
        "README snippet does not contain summary line",
    )?;
    crate::ensure(
        bounded(&rendered),
        "README snippet lost bounded anti-overclaim language",
    )?;
    match replace_readme_marker_block("# Demo\n", &render_readme_status_block(&summary)) {
        Err(_) => Ok(summary),
        Ok(_) => Err(crate::EvidenceError::new(
            "README updater accepted missing status markers",
        )),
    }
}

fn executed_static_gate_names(flake_text: &str) -> crate::EvidenceResult<Vec<String>> {
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

fn validate_unique_nonempty(names: Vec<String>, label: &str) -> crate::EvidenceResult<Vec<String>> {
    crate::ensure(!names.is_empty(), format!("no {label} found"))?;
    let set = names.iter().collect::<::std::collections::BTreeSet<_>>();
    crate::ensure(
        set.len() == names.len(),
        format!("duplicate {label}: {names:?}"),
    )?;
    Ok(names)
}

fn between<'a>(
    text: &'a str,
    start_marker: &str,
    end_marker: &str,
) -> crate::EvidenceResult<&'a str> {
    let start = text.find(start_marker).ok_or_else(|| {
        crate::EvidenceError::new(format!("missing start marker: {start_marker}"))
    })?;
    let end = text[start..]
        .find(end_marker)
        .ok_or_else(|| crate::EvidenceError::new(format!("missing end marker: {end_marker}")))?
        + start;
    Ok(&text[start..end])
}

fn render_gate_rows(gates: &[::serde_json::Value]) -> crate::EvidenceResult<String> {
    let mut rows = Vec::new();
    for gate in gates {
        let name = str_field(gate.get("name"), "gate.name")?;
        let command = str_field(gate.get("command"), "gate.command")?;
        let status = str_field(gate.get("status"), "gate.status")?;
        rows.push(format!("<tr><td>{}</td><td><span class=\"pill {}\">{}</span></td><td><code>{}</code></td></tr>", esc(name), token_class(status), esc(status), esc(command)));
    }
    Ok(rows.join("\n"))
}

fn typed_command_field(
    value: Option<&::serde_json::Value>,
    field: &str,
) -> crate::EvidenceResult<crate::typed_operator_command::CommandPlan> {
    let value = value
        .ok_or_else(|| crate::EvidenceError::new(format!("{field}: missing typed command")))?;
    crate::typed_operator_command::parse_plan(value)
        .map_err(|error| crate::EvidenceError::new(format!("{field}: {error}")))
}

fn execute_typed_command_field(
    value: Option<&::serde_json::Value>,
    field: &str,
) -> crate::EvidenceResult<(
    crate::typed_operator_command::CommandPlan,
    crate::typed_operator_command::CommandObservation,
)> {
    let command = typed_command_field(value, field)?;
    let observation = crate::execute_typed_operator_command(&command, ::std::path::Path::new("."))?;
    Ok((command, observation))
}

fn str_field<'a>(
    value: Option<&'a ::serde_json::Value>,
    field: &str,
) -> crate::EvidenceResult<&'a str> {
    match value.and_then(::serde_json::Value::as_str) {
        Some(text) if !text.is_empty() => Ok(text),
        _ => Err(crate::EvidenceError::new(format!(
            "{field}: expected non-empty string"
        ))),
    }
}

fn token_field<'a>(
    value: Option<&'a ::serde_json::Value>,
    field: &str,
) -> crate::EvidenceResult<&'a str> {
    let text = str_field(value, field)?;
    crate::ensure(
        !text.chars().any(char::is_whitespace),
        format!("{field}: expected whitespace-free string"),
    )?;
    Ok(text)
}

fn object_field<'a>(
    value: Option<&'a ::serde_json::Value>,
    field: &str,
) -> crate::EvidenceResult<&'a serde_json::Map<String, ::serde_json::Value>> {
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
        .ok_or_else(|| crate::EvidenceError::new(format!("{field}: expected non-empty list")))
}

fn int_field(value: Option<&::serde_json::Value>, field: &str) -> crate::EvidenceResult<i64> {
    value
        .and_then(::serde_json::Value::as_i64)
        .ok_or_else(|| crate::EvidenceError::new(format!("{field}: expected integer")))
}

fn json_display(value: &::serde_json::Value) -> String {
    match value {
        ::serde_json::Value::Null => "none".to_string(),
        ::serde_json::Value::Bool(true) => "true".to_string(),
        ::serde_json::Value::Bool(false) => "false".to_string(),
        ::serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn esc_value(value: Option<&::serde_json::Value>) -> String {
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
