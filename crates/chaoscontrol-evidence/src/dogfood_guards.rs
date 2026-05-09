use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::Value;

use crate::{ensure, AcceptedWorkloadProofs, EvidenceError, EvidenceResult, REQUIRED_REPLAY_CLASS};

pub const DEFAULT_MAX_DOGFOOD_ARTIFACT_BYTES: u64 = 50 * 1024 * 1024;

pub fn check_dogfood_artifact_sizes(
    root: impl AsRef<Path>,
    max_bytes: u64,
) -> EvidenceResult<String> {
    ensure(max_bytes > 0, "--max-bytes must be positive")?;
    let root = root.as_ref();
    if !root.exists() {
        return Ok(format!(
            "dogfood artifact size guard: {} absent; nothing to scan",
            root.display()
        ));
    }
    ensure(
        root.is_dir(),
        format!(
            "dogfood artifact size guard: {} is not a directory",
            root.display()
        ),
    )?;
    let mut scanned = 0usize;
    let mut oversized = Vec::new();
    scan_files(root, &mut |path| {
        scanned += 1;
        let size = path.metadata()?.len();
        if size > max_bytes {
            oversized.push((path.to_path_buf(), size));
        }
        Ok(())
    })?;
    if !oversized.is_empty() {
        let mut message = format!(
            "dogfood artifact size guard failed: {} file(s) exceed {max_bytes} bytes",
            oversized.len()
        );
        for (path, size) in oversized {
            message.push_str(&format!("\n  {}: {size} bytes", path.display()));
        }
        message.push_str("\nUse chunked snapshot evidence (<snapshot>.chunks.json + .partNN), artifact summaries, or external storage instead of committing large blobs.");
        return Err(EvidenceError::new(message));
    }
    Ok(format!(
        "dogfood artifact size guard ok: scanned {scanned} file(s), max allowed {max_bytes} bytes"
    ))
}

pub fn validate_accepted_dogfood_config(
    config_path: impl AsRef<Path>,
    expectations_path: impl AsRef<Path>,
    manifest_path: impl AsRef<Path>,
) -> EvidenceResult<String> {
    let config_path = config_path.as_ref();
    let expectations_path = expectations_path.as_ref();
    let manifest_path = manifest_path.as_ref();
    let config = load_json(config_path)?;
    let expectations_root = load_json(expectations_path)?;
    let manifest = AcceptedWorkloadProofs::from_path(manifest_path)?;

    let Some(config) = config.as_object() else {
        return Err(EvidenceError::new("config: expected workload object"));
    };
    let expectations = expectations_root
        .get("workloads")
        .and_then(Value::as_object)
        .ok_or_else(|| EvidenceError::new("expectations: missing workloads object"))?;
    let mut errors = Vec::new();

    let proof_workloads = manifest
        .proofs
        .iter()
        .map(|proof| proof.workload.clone())
        .collect::<BTreeSet<_>>();
    let config_workloads = config.keys().cloned().collect::<BTreeSet<_>>();
    let expectation_workloads = expectations.keys().cloned().collect::<BTreeSet<_>>();

    push_missing(
        &mut errors,
        "missing wrapper config for accepted proof workloads",
        proof_workloads.difference(&config_workloads),
    );
    push_missing(
        &mut errors,
        "wrapper config has no accepted proof manifest entry",
        config_workloads.difference(&proof_workloads),
    );
    push_missing(
        &mut errors,
        "wrapper config has no expectation lock entry",
        config_workloads.difference(&expectation_workloads),
    );
    push_missing(
        &mut errors,
        "expectation lock has no wrapper config entry",
        expectation_workloads.difference(&config_workloads),
    );

    for workload in config_workloads.intersection(&expectation_workloads) {
        let Some(cfg) = config.get(workload).and_then(Value::as_object) else {
            errors.push(format!(
                "{workload}: config and expectation must be objects"
            ));
            continue;
        };
        let Some(exp) = expectations.get(workload).and_then(Value::as_object) else {
            errors.push(format!(
                "{workload}: config and expectation must be objects"
            ));
            continue;
        };
        if cfg.get("assertion_id") != exp.get("assertion_id") {
            errors.push(format!(
                "{workload}: wrapper assertion_id {} != expectation {}",
                display_value(cfg.get("assertion_id")),
                display_value(exp.get("assertion_id"))
            ));
        }
        if cfg.get("expectation") != expectations.get(workload) {
            errors.push(format!(
                "{workload}: Nix-generated embedded expectation differs from lockfile"
            ));
        }
        let runner = exp.get("runner").and_then(Value::as_object);
        if exp.get("runner").is_some() && runner.is_none() {
            errors.push(format!("{workload}: expectation runner must be an object"));
        }
        let runner = runner.cloned().unwrap_or_default();
        let expected_fail_after_values = int_list(
            runner.get("fail_after_values"),
            &format!("{workload}: expectation runner.fail_after_values"),
            &mut errors,
        );
        let cfg_fail_after_values = int_list(
            cfg.get("fail_after_values"),
            &format!("{workload}: wrapper fail_after_values"),
            &mut errors,
        );
        if !expected_fail_after_values.is_empty()
            && !cfg_fail_after_values.is_empty()
            && cfg_fail_after_values != expected_fail_after_values
        {
            errors.push(format!(
                "{workload}: wrapper fail_after_values {cfg_fail_after_values:?} != expectation {expected_fail_after_values:?}"
            ));
        }
        if let Some(expected_max_attempts) = runner.get("max_attempts") {
            if cfg.get("max_attempts") != Some(expected_max_attempts) {
                errors.push(format!(
                    "{workload}: wrapper max_attempts {} != expectation {}",
                    display_value(cfg.get("max_attempts")),
                    display_value(Some(expected_max_attempts))
                ));
            }
        }
        let template = cfg
            .get("cmdline_template")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let required_probe = format!(
            "{}=snapshot_replay_probe",
            exp.get("probe_key")
                .and_then(Value::as_str)
                .unwrap_or_default()
        );
        let required_fail_after = format!(
            "{}={{fail_after}}",
            exp.get("fail_after_key")
                .and_then(Value::as_str)
                .unwrap_or_default()
        );
        if !template.contains(&required_probe) || !template.contains(&required_fail_after) {
            errors.push(format!(
                "{workload}: cmdline_template does not contain locked probe/fail_after keys"
            ));
        }
        let expected = exp.get("expected").and_then(Value::as_object);
        if exp.get("expected").is_some() && expected.is_none() {
            errors.push(format!(
                "{workload}: expectation expected must be an object"
            ));
        }
        let expected = expected.cloned().unwrap_or_default();
        if expected.get("accepted") != Some(&Value::Bool(true)) {
            errors.push(format!(
                "{workload}: expectation expected.accepted must be true"
            ));
        }
        if expected.get("replay_class").and_then(Value::as_str) != Some(REQUIRED_REPLAY_CLASS) {
            errors.push(format!(
                "{workload}: expectation replay_class {} != {REQUIRED_REPLAY_CLASS}",
                display_value(expected.get("replay_class"))
            ));
        }
        let expected_values = int_list(
            expected.get("fail_after_values"),
            &format!("{workload}: expectation expected.fail_after_values"),
            &mut errors,
        );
        if !expected_values.is_empty()
            && !expected_fail_after_values.is_empty()
            && expected_values != expected_fail_after_values
        {
            errors.push(format!(
                "{workload}: expected fail_after_values {expected_values:?} != runner fail_after_values {expected_fail_after_values:?}"
            ));
        }
    }

    let repo_root = manifest_path
        .parent()
        .and_then(Path::parent)
        .unwrap_or(Path::new("."));
    let config_by_workload: BTreeMap<_, _> = config.iter().collect();
    for proof in &manifest.proofs {
        let Some(cfg) = config_by_workload
            .get(&proof.workload)
            .and_then(|value| value.as_object())
        else {
            continue;
        };
        let exp = expectations
            .get(&proof.workload)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        if cfg.get("assertion_id") != Some(&Value::from(proof.assertion_id)) {
            errors.push(format!(
                "{}: wrapper assertion_id {} != manifest {}",
                proof.workload,
                display_value(cfg.get("assertion_id")),
                proof.assertion_id
            ));
        }
        if !exp.is_empty() && exp.get("assertion_id") != Some(&Value::from(proof.assertion_id)) {
            errors.push(format!(
                "{}: expectation assertion_id {} != manifest {}",
                proof.workload,
                display_value(exp.get("assertion_id")),
                proof.assertion_id
            ));
        }
        let summary_path = repo_root.join(&proof.evidence_dir).join(&proof.summary);
        if !summary_path.is_file() {
            errors.push(format!(
                "{}: missing accepted summary {}",
                proof.workload,
                summary_path.display()
            ));
            continue;
        }
        let summary = match load_json(&summary_path) {
            Ok(value) => value,
            Err(err) => {
                errors.push(format!("{}: {err}", proof.workload));
                continue;
            }
        };
        if summary.get("accepted") != Some(&Value::Bool(true)) {
            errors.push(format!("{}: summary is not accepted=true", proof.workload));
        }
        let verdict = summary.get("verdict").and_then(Value::as_object);
        let replay_class = verdict
            .and_then(|value| value.get("replay_class"))
            .and_then(Value::as_str);
        if replay_class != Some(REQUIRED_REPLAY_CLASS) {
            errors.push(format!(
                "{}: summary replay_class {} != {REQUIRED_REPLAY_CLASS}",
                proof.workload,
                display_value(verdict.and_then(|value| value.get("replay_class")))
            ));
        }
        let min_depth = exp
            .get("expected")
            .and_then(Value::as_object)
            .and_then(|value| value.get("min_replay_parent_depth"))
            .and_then(Value::as_i64)
            .unwrap_or(1);
        let depth = verdict
            .and_then(|value| value.get("replay_parent_depth"))
            .and_then(Value::as_i64);
        if depth.is_none_or(|depth| depth < min_depth) {
            errors.push(format!(
                "{}: summary replay_parent_depth {} < expected {min_depth}",
                proof.workload,
                depth
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "None".to_string())
            ));
        }
        if summary_fail_after(&summary, &proof.workload).is_none() {
            errors.push(format!(
                "{}: summary has no snapshot probe fail-after field",
                proof.workload
            ));
        }
    }

    if !errors.is_empty() {
        return Err(EvidenceError::new(
            errors
                .into_iter()
                .map(|error| format!("accepted-dogfood-config: {error}"))
                .collect::<Vec<_>>()
                .join("\n"),
        ));
    }
    Ok(format!(
        "accepted-dogfood-config: {} workloads match expectation lockfile and deterministic wrapper config",
        manifest.proofs.len()
    ))
}

pub fn run_dogfood_guards_selftest() -> EvidenceResult<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    std::fs::create_dir_all(root.join("dogfood-results/proof"))?;
    std::fs::write(root.join("dogfood-results/proof/snapshot.bin"), b"12345")?;
    let ok = check_dogfood_artifact_sizes(root.join("dogfood-results"), 10)?;
    ensure(
        ok.contains("scanned 1 file"),
        "artifact-size selftest scan count drifted",
    )?;
    let err = check_dogfood_artifact_sizes(root.join("dogfood-results"), 4)
        .expect_err("oversized fixture rejected");
    ensure(
        err.message().contains("exceed 4 bytes"),
        "artifact-size selftest lost failure detail",
    )?;
    Ok(())
}

fn scan_files(
    root: &Path,
    visit: &mut impl FnMut(&Path) -> EvidenceResult<()>,
) -> EvidenceResult<()> {
    let mut entries = std::fs::read_dir(root)?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            scan_files(&path, visit)?;
        } else if path.is_file() {
            visit(&path)?;
        }
    }
    Ok(())
}

fn load_json(path: &Path) -> EvidenceResult<Value> {
    let text = std::fs::read_to_string(path)
        .map_err(|err| EvidenceError::new(format!("{}: {err}", path.display())))?;
    serde_json::from_str(&text).map_err(Into::into)
}

fn summary_fail_after(summary: &Value, workload: &str) -> Option<i64> {
    summary
        .get("snapshot_probe_fail_after")
        .and_then(Value::as_i64)
        .or_else(|| {
            summary
                .get(format!(
                    "{}_snapshot_probe_fail_after",
                    workload.replace('-', "_")
                ))
                .and_then(Value::as_i64)
        })
}

fn int_list(value: Option<&Value>, field: &str, errors: &mut Vec<String>) -> Vec<i64> {
    let Some(Value::Array(values)) = value else {
        errors.push(format!("{field}: expected non-empty integer list"));
        return Vec::new();
    };
    if values.is_empty() {
        errors.push(format!("{field}: expected non-empty integer list"));
        return Vec::new();
    }
    let mut result = Vec::new();
    for item in values {
        if let Some(value) = item.as_i64().filter(|_| !item.is_boolean()) {
            result.push(value);
        } else {
            errors.push(format!(
                "{field}: expected integer list, got {}",
                display_value(Some(item))
            ));
            return Vec::new();
        }
    }
    result
}

fn push_missing<'a>(
    errors: &mut Vec<String>,
    label: &str,
    values: impl Iterator<Item = &'a String>,
) {
    let values = values.cloned().collect::<Vec<_>>();
    if !values.is_empty() {
        errors.push(format!("{label}: {}", values.join(", ")));
    }
}

fn display_value(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Null) | None => "None".to_string(),
        Some(other) => other.to_string(),
    }
}
