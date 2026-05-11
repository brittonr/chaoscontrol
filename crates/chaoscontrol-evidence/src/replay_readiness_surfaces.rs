use std::collections::BTreeSet;
use std::path::Path;

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
