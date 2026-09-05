const TRIAGE_DIR: &str = "target/operator-triage";
const MANIFEST_PATH: &str = "dogfood-results/accepted-workload-proofs.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriageReceiptSource<'a> {
    Path(&'a std::path::Path),
    Sample,
}

pub fn render_operator_triage_runbook_path(
    root: impl AsRef<std::path::Path>,
    receipt_source: TriageReceiptSource<'_>,
) -> crate::EvidenceResult<String> {
    let root = root.as_ref();
    let receipt = match receipt_source {
        TriageReceiptSource::Path(path) => read_json_value(path)?,
        TriageReceiptSource::Sample => {
            crate::replay_readiness_surfaces::sample_replay_readiness_receipt(false, "passed")
        }
    };
    render_operator_triage_runbook(root, &receipt)
}

pub fn write_operator_triage_runbook_path(
    root: impl AsRef<std::path::Path>,
    receipt_source: TriageReceiptSource<'_>,
    output: impl AsRef<std::path::Path>,
) -> crate::EvidenceResult<()> {
    let rendered = render_operator_triage_runbook_path(root, receipt_source)?;
    let output = output.as_ref();
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, rendered)?;
    Ok(())
}

pub fn check_operator_triage_runbook_path(
    root: impl AsRef<std::path::Path>,
    receipt_source: TriageReceiptSource<'_>,
    expected_path: impl AsRef<std::path::Path>,
) -> crate::EvidenceResult<()> {
    let expected = render_operator_triage_runbook_path(root, receipt_source)?;
    let expected_path = expected_path.as_ref();
    let actual = std::fs::read_to_string(expected_path).map_err(|err| {
        crate::EvidenceError::new(format!(
            "missing or unreadable operator triage runbook {}: {err}",
            expected_path.display()
        ))
    })?;
    crate::ensure(
        actual == expected,
        "operator triage runbook stale: run `cargo run -p chaoscontrol-evidence --bin replay-readiness-triage -- --root . --sample-receipt --output docs/operator-triage-runbook.md`",
    )
}

pub fn render_operator_triage_runbook(
    root: &std::path::Path,
    receipt: &::serde_json::Value,
) -> crate::EvidenceResult<String> {
    let summary_line = crate::replay_readiness_core::summarize_receipt(receipt)?;
    let selected_workload = selected_workload(receipt)?;
    let manifest =
        crate::AcceptedWorkloadProofs::from_path(root.join(MANIFEST_PATH)).map_err(|err| {
            crate::EvidenceError::new(format!("{}: {err}", root.join(MANIFEST_PATH).display()))
        })?;
    manifest.validate_shape()?;
    let proofs = select_proofs(&manifest, selected_workload)?;
    crate::ensure(
        !proofs.is_empty(),
        "operator triage needs at least one proof",
    )?;

    let has_promotable_proof = proofs
        .iter()
        .any(|proof| crate::validate_workload_proof(root, proof).is_ok());
    let mut output = String::new();
    output.push_str("# ChaosControl Local Operator Triage Runbook\n\n");
    output.push_str("Generated from a replay-readiness receipt and `dogfood-results/accepted-workload-proofs.json`. Do not scrape `run.log`, `reproduce.log`, or temporary VM logs for the triage decision. Use only the bounded artifacts and status below.\n\n");
    output.push_str("## Receipt entry point\n\n");
    output.push_str(&format!("- Summary: `{summary_line}`\n"));
    output.push_str(&format!(
        "- Selected workload: `{}`\n",
        selected_workload.unwrap_or("all committed proofs")
    ));
    output.push_str("- Scope: bounded committed replay/evidence readiness; not hosted product parity and not universal determinism.\n\n");
    output.push_str("## Triage steps\n\n");
    output.push_str(
        "1. Open the readiness receipt and dashboard/summary artifacts for status only.\n",
    );
    if has_promotable_proof {
        output.push_str("2. Confirm that each selected bug and verdict has exact admitted v2 assertion identity.\n3. Re-run reproduce and minimize only with the catalog-bound commands shown below.\n4. Record the operator decision. Keep raw logs local unless a concise hash-bound receipt explicitly promotes them.\n\n");
    } else {
        output.push_str("2. Treat every listed bug, verdict, and snapshot as historical diagnostic data.\n3. Do not run reproduction or minimization for these ID-only carriers.\n4. Record `blocked-assertion-identity` until fresh admitted v2 KVM evidence exists.\n\n");
    }
    output.push_str("## Workloads\n\n");

    for proof in proofs {
        output.push_str(&render_proof_section(root, proof)?);
    }
    while output.ends_with("\n\n") {
        output.pop();
    }

    crate::ensure(
        !contains_raw_log_reference(&output),
        "operator triage runbook must not require raw-log scraping",
    )?;
    Ok(output)
}

fn render_proof_section(
    root: &std::path::Path,
    proof: &crate::AcceptedWorkloadProof,
) -> crate::EvidenceResult<String> {
    let evidence_dir = root.join(&proof.evidence_dir);
    let summary: crate::AcceptedVerdictSummary = read_json(&evidence_dir.join(&proof.summary))?;
    let bug: crate::BugRecord = read_json(&evidence_dir.join(&proof.bug))?;
    let verdict: crate::ReplayVerdict = read_json(&evidence_dir.join(&proof.verdict))?;
    crate::ensure(
        summary.accepted && summary.reproduce_exit_status == 0 && summary.export_exit_status == 0,
        format!("{} summary is not accepted/reproduced", proof.workload),
    )?;
    crate::ensure(
        bug.replay_parent_depth > 0 && bug.replay_parent_snapshot_ref.is_some(),
        format!("{} bug lacks snapshot-backed replay parent", proof.workload),
    )?;
    crate::ensure(
        verdict.replay_class == crate::REQUIRED_REPLAY_CLASS && verdict.reproduced,
        format!("{} verdict is not accepted replay evidence", proof.workload),
    )?;

    let bug_path = format!("{}/{}", proof.evidence_dir, proof.bug);
    let verdict_path = format!("{}/{}", proof.evidence_dir, proof.verdict);
    let snapshot_path = format!("{}/{}", proof.evidence_dir, proof.snapshot);
    if let Err(error) = crate::validate_workload_proof(root, proof) {
        return Ok(format!(
            "### `{}` — blocked assertion identity\n\n- Historical bug: `{bug_path}`\n- Historical replay verdict: `{verdict_path}`\n- Historical snapshot: `{snapshot_path}`\n- Status: `blocked-assertion-identity`\n- Blocker: {}\n\nDo not reproduce, minimize, or promote this ID-only carrier. Generate fresh admitted v2 KVM evidence first.\n\n",
            proof.workload,
            error.message()
        ));
    }
    let triage_verdict = format!("{TRIAGE_DIR}/{}-replay-verdict.json", proof.workload);
    let minimized = format!("{TRIAGE_DIR}/{}-minimized-bug.json", proof.workload);
    let decision = format!("{TRIAGE_DIR}/{}-decision.json", proof.workload);
    let decision_json = ::serde_json::json!({
        "schema_version": 1,
        "workload": proof.workload,
        "assertion_id": proof.assertion_id,
        "bug": bug_path,
        "replay_verdict": triage_verdict,
        "minimized_bug": minimized,
        "decision": "accepted|needs-refresh|blocked",
        "reason": "operator note",
        "raw_log_scraping": false
    });
    let decision_json = serde_json::to_string_pretty(&decision_json)?;

    let mut output = String::new();
    output.push_str(&format!("### `{}`\n\n", proof.workload));
    output.push_str(&format!("- Assertion: `{}`\n", proof.assertion_id));
    output.push_str(&format!(
        "- Evidence directory: `{}/`\n",
        proof.evidence_dir
    ));
    output.push_str(&format!("- Bug: `{bug_path}`\n"));
    output.push_str(&format!("- Replay verdict: `{verdict_path}`\n"));
    output.push_str(&format!(
        "- Snapshot artifact or chunk manifest: `{snapshot_path}`\n"
    ));
    output.push_str(&format!(
        "- Accepted summary: `{}/{}`\n",
        proof.evidence_dir, proof.summary
    ));
    output.push_str(&format!(
        "- Replay class/depth: `{}` / `{}`\n\n",
        verdict.replay_class, verdict.replay_parent_depth
    ));
    output.push_str("Reproduce from committed artifacts:\n\n");
    output.push_str("```bash\n");
    output.push_str(&format!("mkdir -p {TRIAGE_DIR}\n"));
    output.push_str(&format!(
        "{}\n",
        rewrite_verdict_output(&verdict.command.command, &triage_verdict)
    ));
    output.push_str("```\n\n");
    output.push_str(
        "Minimize using the same kernel/initrd/VM options as the reproduce command above:\n\n",
    );
    output.push_str("```bash\n");
    output.push_str(&format!("cargo run --release --bin chaoscontrol-explore -- minimize --bug {bug_path} --output {minimized}\n"));
    output.push_str("```\n\n");
    output.push_str("Record the operator decision:\n\n");
    output.push_str("```bash\n");
    output.push_str(&format!(
        "cat > {decision} <<'JSON'\n{decision_json}\nJSON\n"
    ));
    output.push_str("```\n\n");
    Ok(output)
}

fn selected_workload(receipt: &::serde_json::Value) -> crate::EvidenceResult<Option<&str>> {
    let dogfood = receipt
        .get("dogfood")
        .and_then(::serde_json::Value::as_object)
        .ok_or_else(|| crate::EvidenceError::new("receipt.dogfood must be an object"))?;
    match dogfood.get("selected_workload") {
        Some(::serde_json::Value::String(workload)) if !workload.is_empty() => {
            Ok(Some(workload.as_str()))
        }
        Some(::serde_json::Value::Null) | None => Ok(None),
        _ => Err(crate::EvidenceError::new(
            "receipt.dogfood.selected_workload must be string or null",
        )),
    }
}

fn select_proofs<'a>(
    manifest: &'a crate::AcceptedWorkloadProofs,
    selected_workload: Option<&str>,
) -> crate::EvidenceResult<Vec<&'a crate::AcceptedWorkloadProof>> {
    if let Some(workload) = selected_workload {
        let proof = manifest
            .proofs
            .iter()
            .find(|proof| proof.workload == workload)
            .ok_or_else(|| {
                crate::EvidenceError::new(format!(
                    "selected workload {workload:?} missing from accepted proof manifest"
                ))
            })?;
        Ok(vec![proof])
    } else {
        Ok(manifest.proofs.iter().collect())
    }
}

fn rewrite_verdict_output(command: &str, triage_verdict: &str) -> String {
    if let Some((prefix, _)) = command.rsplit_once(" --verdict-output ") {
        format!("{prefix} --verdict-output {triage_verdict}")
    } else {
        format!("{command} --verdict-output {triage_verdict}")
    }
}

fn contains_raw_log_reference(text: &str) -> bool {
    [
        "cat run.log",
        "cat reproduce.log",
        "grep run.log",
        "grep reproduce.log",
        "tail run.log",
        "tail reproduce.log",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

fn read_json_value(path: &std::path::Path) -> crate::EvidenceResult<::serde_json::Value> {
    let input = std::fs::read_to_string(path)
        .map_err(|err| crate::EvidenceError::new(format!("{}: {err}", path.display())))?;
    serde_json::from_str(&input).map_err(Into::into)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> crate::EvidenceResult<T> {
    let input = std::fs::read_to_string(path)
        .map_err(|err| crate::EvidenceError::new(format!("{}: {err}", path.display())))?;
    serde_json::from_str(&input).map_err(Into::into)
}

pub fn committed_operator_triage_runbook_path(
    root: impl AsRef<std::path::Path>,
) -> std::path::PathBuf {
    root.as_ref().join("docs/operator-triage-runbook.md")
}
