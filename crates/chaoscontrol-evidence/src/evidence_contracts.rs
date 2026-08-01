use std::collections::BTreeSet;
use std::path::{Component, Path};
use std::process::{Command, Stdio};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    EvidenceError, EvidenceResult, SUPPORTED_SNAPSHOT_CODECS, SUPPORTED_SNAPSHOT_SCHEMA_VERSIONS,
};

pub const EVIDENCE_CONTRACTS_SUCCESS: &str =
    "evidence contracts ok: nickel examples, dogfood receipt, positive fixtures, negative fixtures";

const STATUSES: [&str; 5] = [
    "accepted",
    "partial",
    "known-gap",
    "invalid",
    "raw-log-only",
];
const REPLAY_CLASSES: [&str; 8] = [
    "snapshot_backed_reproduced",
    "snapshot_backed_not_reproduced",
    "schedule_only_replay_gap",
    "missing_snapshot_ref",
    "missing_snapshot_artifact",
    "invalid_snapshot_digest",
    "no_bug_found",
    "replay_error",
];
const MAX_EVIDENCE_JSON_BYTES: u64 = 16 * 1024 * 1024;
const LEGACY_REPLAY_VERDICT_SCHEMA_VERSION: u64 = 1;
const SUBSTITUTED_ASSERTION_ALIAS: u64 = 8;

const SNAPSHOT_STATUSES: [&str; 6] = [
    "not_required",
    "missing_ref",
    "valid",
    "missing_artifact",
    "invalid_digest",
    "invalid_ref",
];

pub fn check_evidence_contracts(root: impl AsRef<Path>) -> EvidenceResult<&'static str> {
    let root = root.as_ref();
    run_nickel_examples(root)?;
    check_evidence_contract_fixtures(root)?;
    Ok(EVIDENCE_CONTRACTS_SUCCESS)
}

pub fn check_evidence_contract_fixtures(root: impl AsRef<Path>) -> EvidenceResult<()> {
    let root = root.as_ref();
    let contracts = root.join("contracts/evidence");
    let dogfood = root.join("dogfood-results/raft-20260506-095025");

    validate_run_config(&load_json(&dogfood.join("run-config.json"))?)?;
    validate_bug_report(&load_json(&dogfood.join("bug_0.json"))?)?;
    validate_assertion_summary(&load_json(&dogfood.join("assertions.json"))?)?;
    let receipt = load_json(&dogfood.join("receipt.json"))?;
    validate_receipt_with_root(&receipt, root, true)?;
    validate_markdown_receipt(&receipt, &dogfood)?;

    let valid = contracts.join("fixtures/valid");
    validate_run_config(&load_json(&valid.join("run-config.valid.json"))?)?;
    validate_receipt(&load_json(&valid.join("receipt.known-gap.valid.json"))?)?;
    let legacy_bug = load_json(&valid.join("bug-report.valid.json"))?;
    validate_bug_report(&legacy_bug)?;
    ensure(
        validate_bug_report_for_replay(&legacy_bug).is_err(),
        "legacy assertion ID-only bug unexpectedly qualified for replay",
    )?;
    let identity_bug = load_json(&valid.join("bug-report.identity.valid.json"))?;
    validate_bug_report(&identity_bug)?;
    validate_bug_report_for_replay(&identity_bug)?;
    let mut substituted_bug = identity_bug.clone();
    substituted_bug["assertion_id"] = Value::from(SUBSTITUTED_ASSERTION_ALIAS);
    ensure(
        validate_bug_report_for_replay(&substituted_bug).is_err(),
        "substituted bug alias unexpectedly qualified for replay",
    )?;
    let mut forged_bug = identity_bug.clone();
    forged_bug["assertion_identity"]["descriptor"]["message"] = Value::from("forged descriptor");
    ensure(
        validate_bug_report_for_replay(&forged_bug).is_err(),
        "forged bug descriptor unexpectedly qualified for replay",
    )?;
    let mut extended_bug = identity_bug;
    extended_bug["unreviewed_authority"] = Value::from(true);
    ensure(
        validate_bug_report_for_replay(&extended_bug).is_err(),
        "unknown bug field unexpectedly qualified for replay",
    )?;
    validate_snapshot_ref(&load_json(&valid.join("snapshot-ref.valid.json"))?)?;
    let replay_verdict = load_json(&valid.join("replay-verdict.snapshot-backed.valid.json"))?;
    validate_replay_verdict_with_options(&replay_verdict, true, false, root)?;
    let mut extended_verdict = replay_verdict;
    extended_verdict["unreviewed_authority"] = Value::from(true);
    ensure(
        validate_replay_verdict_with_options(&extended_verdict, true, false, root).is_err(),
        "unknown replay verdict field unexpectedly qualified for replay",
    )?;
    validate_assertion_summary(&load_json(&valid.join("assertions.valid.json"))?)?;
    let identity_summary = load_json(&valid.join("assertions.identity.valid.json"))?;
    validate_assertion_summary(&identity_summary)?;
    validate_assertion_summary_for_promotion(&identity_summary)?;

    let invalid = contracts.join("fixtures/invalid");
    expect_invalid(
        &invalid.join("run-config.zero-vms.invalid.json"),
        validate_run_config,
    )?;
    expect_invalid(
        &invalid.join("receipt.missing-hash.invalid.json"),
        validate_receipt,
    )?;
    expect_invalid(
        &invalid.join("receipt.missing-replay-attempt.invalid.json"),
        validate_receipt,
    )?;
    expect_invalid(
        &invalid.join("assertions.bad-verdict.invalid.json"),
        validate_assertion_summary,
    )?;
    expect_invalid(
        &invalid.join("assertions.identity-conflict.invalid.json"),
        validate_assertion_summary,
    )?;
    expect_invalid(
        &invalid.join("assertions.legacy-promotion.invalid.json"),
        validate_assertion_summary_for_promotion,
    )?;
    expect_invalid(
        &invalid.join("bug-report.missing-schedule.invalid.json"),
        validate_bug_report,
    )?;
    expect_invalid(
        &invalid.join("bug-report.missing-snapshot-ref.invalid.json"),
        validate_bug_report,
    )?;
    expect_invalid(
        &invalid.join("receipt.missing-deterministic-context.invalid.json"),
        validate_receipt,
    )?;
    expect_invalid(
        &invalid.join("receipt.missing-snapshot-ref.invalid.json"),
        validate_receipt,
    )?;
    expect_invalid(
        &invalid.join("snapshot-ref.path-escape.invalid.json"),
        validate_snapshot_ref,
    )?;
    expect_invalid(
        &invalid.join("snapshot-ref.unsupported-codec.invalid.json"),
        validate_snapshot_ref,
    )?;
    expect_invalid(
        &invalid.join("snapshot-ref.incompatible-schema.invalid.json"),
        validate_snapshot_ref,
    )?;
    expect_invalid(
        &invalid.join("snapshot-ref.wrong-hash.invalid.json"),
        |value| validate_snapshot_ref_with_root(value, &invalid, true),
    )?;
    expect_invalid(
        &invalid.join("snapshot-ref.missing.invalid.json"),
        |value| validate_snapshot_ref_with_root(value, &invalid, true),
    )?;
    expect_invalid(
        &invalid.join("snapshot-ref.corrupt.invalid.json"),
        |value| validate_snapshot_ref_with_root(value, &invalid, true),
    )?;
    expect_invalid(
        &invalid.join("receipt.stale-artifact.invalid.json"),
        |value| validate_receipt_with_root(value, root, true),
    )?;

    for fixture in [
        "replay-verdict.schedule-only-not-proof.invalid.json",
        "replay-verdict.missing-snapshot-ref.invalid.json",
        "replay-verdict.missing-snapshot-artifact.invalid.json",
        "replay-verdict.invalid-snapshot-digest.invalid.json",
        "replay-verdict.snapshot-not-reproduced.invalid.json",
        "replay-verdict.no-bug-found.invalid.json",
        "replay-verdict.replay-error.invalid.json",
        "replay-verdict.schema-v1-accepted.invalid.json",
    ] {
        let path = invalid.join(fixture);
        validate_replay_verdict(&load_json(&path)?)?;
        expect_invalid(&path, |value| {
            validate_replay_verdict_with_options(value, true, false, root)
        })?;
    }

    Ok(())
}

pub fn run_nickel_examples(root: impl AsRef<Path>) -> EvidenceResult<()> {
    let root = root.as_ref();
    let command = nickel_command().ok_or_else(|| {
        EvidenceError::new("neither nickel nor nix is available for Nickel export checks")
    })?;
    for rel in [
        "examples/raft-run-config.ncl",
        "examples/raft-dogfood-receipt.ncl",
        "examples/raft-bug-report.ncl",
        "examples/bug-report-identity.ncl",
        "examples/raft-assertion-summary.ncl",
        "examples/assertion-summary-logical-keys.ncl",
        "examples/assertion-summary-no-alias.ncl",
        "examples/assertion-summary-null-alias.ncl",
        "examples/raft-replay-verdict.ncl",
    ] {
        let status = Command::new(&command[0])
            .args(&command[1..])
            .arg(root.join("contracts/evidence").join(rel))
            .current_dir(root)
            .stdout(Stdio::null())
            .status()
            .map_err(|err| {
                EvidenceError::new(format!("failed to run Nickel export for {rel}: {err}"))
            })?;
        ensure(
            status.success(),
            format!("Nickel export failed for {rel}: {status}"),
        )?;
    }
    for rel in [
        "fixtures/invalid/bug-report.alias-substitution.invalid.ncl",
        "fixtures/invalid/bug-report.legacy-descriptor.invalid.ncl",
        "fixtures/invalid/assertions.nickel-ascii-control.invalid.ncl",
        "fixtures/invalid/assertions.nickel-automatic-source-mismatch.invalid.ncl",
        "fixtures/invalid/assertions.nickel-cardinality.invalid.ncl",
        "fixtures/invalid/assertions.nickel-duplicate-fingerprint.invalid.ncl",
        "fixtures/invalid/assertions.nickel-duplicate-legacy-id.invalid.ncl",
        "fixtures/invalid/assertions.nickel-empty.invalid.ncl",
        "fixtures/invalid/assertions.nickel-extra-assertion.invalid.ncl",
        "fixtures/invalid/assertions.nickel-extra-descriptor.invalid.ncl",
        "fixtures/invalid/assertions.nickel-extra-summary.invalid.ncl",
        "fixtures/invalid/assertions.nickel-fatal-metadata-spoof.invalid.ncl",
        "fixtures/invalid/assertions.nickel-fractional-count.invalid.ncl",
        "fixtures/invalid/assertions.nickel-inconsistent-token.invalid.ncl",
        "fixtures/invalid/assertions.nickel-invalid-source-path.invalid.ncl",
        "fixtures/invalid/assertions.nickel-legacy-accepted.invalid.ncl",
        "fixtures/invalid/assertions.nickel-legacy-alias-mismatch.invalid.ncl",
        "fixtures/invalid/assertions.nickel-legacy-control.invalid.ncl",
        "fixtures/invalid/assertions.nickel-multibyte-boundary.invalid.ncl",
        "fixtures/invalid/assertions.nickel-negative-count.invalid.ncl",
        "fixtures/invalid/assertions.nickel-null-identity.invalid.ncl",
        "fixtures/invalid/assertions.nickel-overflow-count.invalid.ncl",
        "fixtures/invalid/assertions.nickel-redundant-id.invalid.ncl",
        "fixtures/invalid/assertions.nickel-redundant-metadata.invalid.ncl",
        "fixtures/invalid/assertions.nickel-token-cardinality.invalid.ncl",
        "fixtures/invalid/assertions.nickel-uppercase-hex.invalid.ncl",
        "fixtures/invalid/assertions.nickel-verdict.invalid.ncl",
        "fixtures/invalid/assertions.nickel-wrong-logical-key.invalid.ncl",
    ] {
        let status = Command::new(&command[0])
            .args(&command[1..])
            .arg(root.join("contracts/evidence").join(rel))
            .current_dir(root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|err| {
                EvidenceError::new(format!(
                    "failed to run negative Nickel export for {rel}: {err}"
                ))
            })?;
        ensure(
            !status.success(),
            format!("negative Nickel fixture unexpectedly passed: {rel}"),
        )?;
    }
    Ok(())
}

fn nickel_command() -> Option<Vec<String>> {
    if command_exists("nickel") {
        return Some(vec!["nickel".to_string(), "export".to_string()]);
    }
    if command_exists("nix") {
        return Some(vec![
            "nix".to_string(),
            "run".to_string(),
            "nixpkgs#nickel".to_string(),
            "--".to_string(),
            "export".to_string(),
        ]);
    }
    None
}

fn command_exists(name: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| dir.join(name).is_file())
}

pub fn validate_run_config(value: &Value) -> EvidenceResult<()> {
    ensure(value.is_object(), "run-config: expected object")?;
    for key in [
        "schema_version",
        "profile",
        "mode",
        "kernel_path",
        "initrd_path",
        "raw_log_policy",
    ] {
        require_str(value.get(key), &format!("run-config.{key}"))?;
    }
    for key in [
        "num_vms",
        "branch_factor",
        "ticks_per_branch",
        "max_rounds",
        "max_frontier",
        "quantum",
        "bootstrap_budget",
    ] {
        require_pos_int(value.get(key), &format!("run-config.{key}"))?;
    }
    for key in ["seed", "coverage_gpa"] {
        require_num(value.get(key), &format!("run-config.{key}"))?;
    }
    Ok(())
}

pub fn validate_artifact_hash(value: &Value) -> EvidenceResult<()> {
    crate::assertion_evidence_carrier::require_only_fields(
        value,
        &["path", "sha256"],
        "artifact-hash",
    )?;
    require_str(value.get("path"), "artifact-hash.path")?;
    let digest = require_str(value.get("sha256"), "artifact-hash.sha256")?;
    ensure(
        is_prefixed_sha256(digest),
        "artifact-hash.sha256: expected sha256:<64 hex>",
    )
}

pub fn validate_snapshot_ref(value: &Value) -> EvidenceResult<()> {
    validate_snapshot_ref_with_root(value, Path::new("."), false)
}

pub fn validate_snapshot_ref_with_root(
    value: &Value,
    root: &Path,
    check_files: bool,
) -> EvidenceResult<()> {
    crate::assertion_evidence_carrier::require_only_fields(
        value,
        &["store", "digest", "codec", "schema_version", "path"],
        "snapshot-ref",
    )?;
    ensure(
        value.get("store").and_then(Value::as_str) == Some("file-content-addressed"),
        "snapshot-ref.store: expected file-content-addressed",
    )?;
    let digest = require_str(value.get("digest"), "snapshot-ref.digest")?;
    ensure(
        is_prefixed_sha256(digest),
        "snapshot-ref.digest: expected sha256:<64 hex>",
    )?;
    ensure(
        value
            .get("codec")
            .and_then(Value::as_str)
            .is_some_and(|codec| SUPPORTED_SNAPSHOT_CODECS.contains(&codec)),
        "snapshot-ref.codec: unsupported",
    )?;
    ensure(
        value
            .get("schema_version")
            .and_then(Value::as_u64)
            .is_some_and(|version| SUPPORTED_SNAPSHOT_SCHEMA_VERSIONS.contains(&version)),
        "snapshot-ref.schema_version: unsupported",
    )?;
    let path_value = require_str(value.get("path"), "snapshot-ref.path")?;
    let path = Path::new(path_value);
    ensure(
        is_safe_snapshot_path(path),
        "snapshot-ref.path: must stay under snapshots/",
    )?;
    if check_files {
        let full = root.join(path);
        ensure(
            full.exists(),
            format!("snapshot artifact missing: {path_value}"),
        )?;
        ensure(
            sha256_file(&full)? == digest,
            format!("snapshot artifact hash mismatch: {path_value}"),
        )?;
        ensure(
            path_value.ends_with(".snapshot.bin"),
            "snapshot-ref.path: expected .snapshot.bin artifact for binary codec",
        )?;
        ensure(
            full.metadata()
                .map_err(|err| EvidenceError::new(format!("I/O error: {err}")))?
                .len()
                > 0,
            format!("snapshot artifact empty: {path_value}"),
        )?;
    }
    Ok(())
}

pub fn validate_bug_report(value: &Value) -> EvidenceResult<()> {
    crate::assertion_evidence_carrier::require_only_fields(
        value,
        &[
            "bug_id",
            "assertion_id",
            "assertion_identity",
            "assertion_location",
            "schedule",
            "tick",
            "replay_parent_depth",
            "replay_parent_snapshot_ref",
            "dedup_key",
            "schedule_variant",
            "scenario_config",
            "scenario_summary",
        ],
        "bug-report",
    )?;
    for key in [
        "bug_id",
        "assertion_id",
        "tick",
        "replay_parent_depth",
        "dedup_key",
    ] {
        require_num(value.get(key), &format!("bug-report.{key}"))?;
    }
    if value
        .get("replay_parent_depth")
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
        > 0.0
    {
        ensure(
            !value
                .get("replay_parent_snapshot_ref")
                .is_none_or(Value::is_null),
            "bug-report.replay_parent_snapshot_ref: required when replay_parent_depth > 0",
        )?;
    }
    if let Some(snapshot) = value
        .get("replay_parent_snapshot_ref")
        .filter(|value| !value.is_null())
    {
        validate_snapshot_ref(snapshot)?;
    }
    require_str(
        value.get("assertion_location"),
        "bug-report.assertion_location",
    )?;
    let schedule = value
        .get("schedule")
        .ok_or_else(|| EvidenceError::new("bug-report.schedule: expected object"))?;
    crate::assertion_evidence_carrier::require_only_fields(
        schedule,
        &["faults"],
        "bug-report.schedule",
    )?;
    let faults = schedule
        .get("faults")
        .ok_or_else(|| EvidenceError::new("bug-report.schedule.faults: expected list"))?;
    ensure(
        faults.is_array(),
        "bug-report.schedule.faults: expected list",
    )?;
    let assertion_id = value
        .get("assertion_id")
        .and_then(Value::as_u64)
        .ok_or_else(|| EvidenceError::new("bug-report.assertion_id: expected unsigned integer"))?;
    crate::assertion_evidence_carrier::optional_identity(
        value,
        "assertion_identity",
        assertion_id,
        false,
        "bug-report",
    )?;
    Ok(())
}

pub fn validate_bug_report_for_replay(value: &Value) -> EvidenceResult<()> {
    validate_bug_report(value)?;
    let assertion_id = value
        .get("assertion_id")
        .and_then(Value::as_u64)
        .ok_or_else(|| EvidenceError::new("bug-report.assertion_id: expected unsigned integer"))?;
    crate::assertion_evidence_carrier::optional_identity(
        value,
        "assertion_identity",
        assertion_id,
        true,
        "bug-report",
    )?;
    let faults = value
        .get("schedule")
        .and_then(|schedule| schedule.get("faults"))
        .and_then(Value::as_array)
        .ok_or_else(|| EvidenceError::new("bug-report.schedule.faults: expected list"))?;
    ensure(
        !faults.is_empty(),
        "bug-report.schedule.faults: expected at least one fault for replay",
    )?;
    Ok(())
}

pub fn validate_assertion_summary(value: &Value) -> EvidenceResult<()> {
    crate::assertion_summary_identity::validate(value, false).map(|_| ())
}

pub fn validate_assertion_summary_for_promotion(value: &Value) -> EvidenceResult<()> {
    crate::assertion_summary_identity::validate(value, true).map(|_| ())
}

pub fn validate_checkpoint_reference(value: Option<&Value>) -> EvidenceResult<()> {
    let value = value.ok_or_else(|| EvidenceError::new("checkpoint-reference: expected object"))?;
    ensure(value.is_object(), "checkpoint-reference: expected object")?;
    for key in ["path", "digest", "kernel_path", "initrd_path"] {
        require_str(value.get(key), &format!("checkpoint-reference.{key}"))?;
    }
    require_num(value.get("seed"), "checkpoint-reference.seed")?;
    Ok(())
}

pub fn validate_replay_verdict(value: &Value) -> EvidenceResult<()> {
    validate_replay_verdict_with_options(value, false, false, Path::new("."))
}

pub fn validate_replay_verdict_with_options(
    value: &Value,
    accepted_snapshot_proof: bool,
    check_files: bool,
    root: &Path,
) -> EvidenceResult<()> {
    crate::assertion_evidence_carrier::require_only_fields(
        value,
        &[
            "schema_version",
            "run_id",
            "replay_class",
            "reproduced",
            "command",
            "diagnostic",
            "bug_path",
            "bug_id",
            "assertion_id",
            "assertion_identity",
            "replay_parent_depth",
            "snapshot",
            "artifact_hashes",
        ],
        "replay-verdict",
    )?;
    let schema_version = value
        .get("schema_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| EvidenceError::new("replay-verdict.schema_version: expected integer"))?;
    ensure(
        matches!(
            schema_version,
            LEGACY_REPLAY_VERDICT_SCHEMA_VERSION | crate::REPLAY_VERDICT_SCHEMA_VERSION
        ),
        format!("replay-verdict.schema_version: unsupported version {schema_version}"),
    )?;
    require_str(value.get("run_id"), "replay-verdict.run_id")?;
    let replay_class = require_str(value.get("replay_class"), "replay-verdict.replay_class")?;
    ensure(
        REPLAY_CLASSES.contains(&replay_class),
        format!(
            "replay-verdict.replay_class: expected one of {:?}",
            sorted(REPLAY_CLASSES)
        ),
    )?;
    require_bool(value.get("reproduced"), "replay-verdict.reproduced")?;
    let command = value
        .get("command")
        .ok_or_else(|| EvidenceError::new("replay-verdict.command: expected object"))?;
    crate::assertion_evidence_carrier::require_only_fields(
        command,
        &["command", "exit_status"],
        "replay-verdict.command",
    )?;
    require_str(command.get("command"), "replay-verdict.command.command")?;
    require_num(
        command.get("exit_status"),
        "replay-verdict.command.exit_status",
    )?;
    require_str(value.get("diagnostic"), "replay-verdict.diagnostic")?;
    let snapshot = value
        .get("snapshot")
        .ok_or_else(|| EvidenceError::new("replay-verdict.snapshot: expected object"))?;
    crate::assertion_evidence_carrier::require_only_fields(
        snapshot,
        &[
            "status",
            "present",
            "digest_verified",
            "reference",
            "diagnostic",
        ],
        "replay-verdict.snapshot",
    )?;
    ensure(
        snapshot
            .get("status")
            .and_then(Value::as_str)
            .is_some_and(|status| SNAPSHOT_STATUSES.contains(&status)),
        "replay-verdict.snapshot.status: invalid",
    )?;
    require_bool(snapshot.get("present"), "replay-verdict.snapshot.present")?;
    require_bool(
        snapshot.get("digest_verified"),
        "replay-verdict.snapshot.digest_verified",
    )?;
    if let Some(reference) = snapshot.get("reference").filter(|value| !value.is_null()) {
        validate_snapshot_ref(reference)?;
    }
    let hashes = value
        .get("artifact_hashes")
        .and_then(Value::as_array)
        .ok_or_else(|| EvidenceError::new("replay-verdict.artifact_hashes: expected list"))?;
    for item in hashes {
        validate_artifact_hash(item)?;
        if check_files {
            let path_str = item["path"].as_str().unwrap();
            let path = root.join(path_str);
            ensure(
                path.exists(),
                format!("replay-verdict artifact missing: {path_str}"),
            )?;
            ensure(
                sha256_file(&path)? == item["sha256"].as_str().unwrap(),
                format!("replay-verdict artifact hash mismatch: {path_str}"),
            )?;
        }
    }
    if !value.get("bug_path").is_none_or(Value::is_null) {
        require_str(value.get("bug_path"), "replay-verdict.bug_path")?;
        require_num(value.get("bug_id"), "replay-verdict.bug_id")?;
        require_num(value.get("assertion_id"), "replay-verdict.assertion_id")?;
        require_num(
            value.get("replay_parent_depth"),
            "replay-verdict.replay_parent_depth",
        )?;
        if schema_version == crate::REPLAY_VERDICT_SCHEMA_VERSION {
            validate_replay_assertion_identity(value)?;
        }
    }
    if accepted_snapshot_proof {
        ensure(
            schema_version == crate::REPLAY_VERDICT_SCHEMA_VERSION,
            format!(
                "accepted replay proof requires schema_version {}",
                crate::REPLAY_VERDICT_SCHEMA_VERSION
            ),
        )?;
        validate_replay_assertion_identity(value)?;
        ensure(
            replay_class == "snapshot_backed_reproduced",
            format!(
                "accepted replay proof requires snapshot_backed_reproduced, got {replay_class}"
            ),
        )?;
        ensure(
            value.get("reproduced").and_then(Value::as_bool) == Some(true),
            "accepted replay proof requires reproduced=true",
        )?;
        ensure(
            command.get("exit_status").and_then(Value::as_i64) == Some(0),
            "accepted replay proof requires command.exit_status=0",
        )?;
        require_num(
            value.get("assertion_id"),
            "accepted replay proof assertion_id",
        )?;
        require_num(
            value.get("replay_parent_depth"),
            "accepted replay proof replay_parent_depth",
        )?;
        ensure(
            value
                .get("replay_parent_depth")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
                > 0.0,
            "accepted replay proof requires replay_parent_depth > 0",
        )?;
        ensure(
            snapshot.get("status").and_then(Value::as_str) == Some("valid"),
            "accepted replay proof requires valid snapshot",
        )?;
        ensure(
            snapshot.get("digest_verified").and_then(Value::as_bool) == Some(true),
            "accepted replay proof requires snapshot digest verification",
        )?;
        ensure(
            !snapshot.get("reference").is_none_or(Value::is_null),
            "accepted replay proof requires snapshot reference",
        )?;
        ensure(
            !hashes.is_empty(),
            "accepted replay proof requires artifact hashes",
        )?;
    }
    Ok(())
}

fn validate_replay_assertion_identity(value: &Value) -> EvidenceResult<()> {
    let assertion_id = value
        .get("assertion_id")
        .and_then(Value::as_u64)
        .ok_or_else(|| EvidenceError::new("replay-verdict.assertion_id: expected integer"))?;
    let identity_value = value
        .get("assertion_identity")
        .ok_or_else(|| EvidenceError::new("replay-verdict.assertion_identity: required for v2"))?;
    ensure(
        !identity_value.is_null(),
        "replay-verdict.assertion_identity: null is invalid",
    )?;
    let identity = serde_json::from_value::<
        chaoscontrol_protocol::admission::AssertionEvidenceIdentity,
    >(identity_value.clone())
    .map_err(|error| {
        EvidenceError::new(format!(
            "replay-verdict.assertion_identity: invalid carrier: {error}"
        ))
    })?;
    identity
        .validate_compatibility_alias(assertion_id)
        .map_err(|error| {
            EvidenceError::new(format!(
                "replay-verdict.assertion_identity: invalid identity: {error:?}"
            ))
        })
}

pub fn validate_receipt(value: &Value) -> EvidenceResult<()> {
    validate_receipt_with_root(value, Path::new("."), false)
}

pub fn validate_receipt_with_root(
    value: &Value,
    root: &Path,
    check_files: bool,
) -> EvidenceResult<()> {
    ensure(value.is_object(), "receipt: expected object")?;
    for key in [
        "schema_version",
        "git_revision",
        "run_id",
        "command",
        "kernel_path",
        "initrd_path",
    ] {
        require_str(value.get(key), &format!("receipt.{key}"))?;
    }
    require_status(value.get("status"), "receipt.status")?;
    require_status(value.get("acceptance_status"), "receipt.acceptance_status")?;
    let artifacts = value
        .get("artifact_hashes")
        .and_then(Value::as_array)
        .ok_or_else(|| EvidenceError::new("receipt.artifact_hashes: expected non-empty list"))?;
    ensure(
        !artifacts.is_empty(),
        "receipt.artifact_hashes: expected non-empty list",
    )?;
    for artifact in artifacts {
        validate_artifact_hash(artifact)?;
        if check_files {
            let path_str = artifact["path"].as_str().unwrap();
            if !path_str.ends_with("run.log") && !path_str.ends_with("reproduce.log") {
                let path = root.join(path_str);
                ensure(
                    path.exists(),
                    format!("receipt artifact missing: {path_str}"),
                )?;
                ensure(
                    sha256_file(&path)? == artifact["sha256"].as_str().unwrap(),
                    format!("receipt artifact hash mismatch: {path_str}"),
                )?;
            }
        }
    }
    let coverage = value
        .get("assertion_coverage")
        .ok_or_else(|| EvidenceError::new("receipt.assertion_coverage: expected object"))?;
    ensure(
        coverage.is_object(),
        "receipt.assertion_coverage: expected object",
    )?;
    for key in ["registered", "exercised", "passed", "failed", "unexercised"] {
        require_num(
            coverage.get(key),
            &format!("receipt.assertion_coverage.{key}"),
        )?;
    }
    let registered = coverage["registered"].as_f64().unwrap();
    let passed = coverage["passed"].as_f64().unwrap();
    let failed = coverage["failed"].as_f64().unwrap();
    let unexercised = coverage["unexercised"].as_f64().unwrap();
    ensure(
        registered == passed + failed + unexercised,
        "receipt.assertion_coverage counts do not add up",
    )?;
    let bugs = value
        .get("bug_reports")
        .and_then(Value::as_array)
        .ok_or_else(|| EvidenceError::new("receipt.bug_reports: expected list"))?;
    for (idx, bug) in bugs.iter().enumerate() {
        require_str(bug.get("path"), &format!("receipt.bug_reports[{idx}].path"))?;
        require_num(
            bug.get("assertion_id"),
            &format!("receipt.bug_reports[{idx}].assertion_id"),
        )?;
        require_num(bug.get("tick"), &format!("receipt.bug_reports[{idx}].tick"))?;
        require_num(
            bug.get("replay_parent_depth"),
            &format!("receipt.bug_reports[{idx}].replay_parent_depth"),
        )?;
        let depth = bug
            .get("replay_parent_depth")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let context = bug
            .get("replay_context")
            .and_then(Value::as_str)
            .unwrap_or("");
        if depth > 0.0 || context.starts_with("parent-snapshot-required") {
            ensure(
                !bug.get("replay_parent_snapshot_ref").is_none_or(Value::is_null),
                format!("receipt.bug_reports[{idx}].replay_parent_snapshot_ref: required for parent snapshot replay"),
            )?;
        }
        if let Some(snapshot) = bug
            .get("replay_parent_snapshot_ref")
            .filter(|value| !value.is_null())
        {
            let snapshot_root = if check_files {
                let bug_path = Path::new(bug["path"].as_str().unwrap());
                root.join(bug_path.parent().unwrap_or_else(|| Path::new("")))
            } else {
                root.to_path_buf()
            };
            validate_snapshot_ref_with_root(snapshot, &snapshot_root, check_files)?;
        }
        require_str(
            bug.get("replay_context"),
            &format!("receipt.bug_reports[{idx}].replay_context"),
        )?;
        require_status(
            bug.get("replay_status"),
            &format!("receipt.bug_reports[{idx}].replay_status"),
        )?;
        let replay = bug.get("replay_attempt").ok_or_else(|| {
            EvidenceError::new(format!(
                "receipt.bug_reports[{idx}].replay_attempt: expected object"
            ))
        })?;
        ensure(
            replay.is_object(),
            format!("receipt.bug_reports[{idx}].replay_attempt: expected object"),
        )?;
        require_str(
            replay.get("command"),
            &format!("receipt.bug_reports[{idx}].replay_attempt.command"),
        )?;
        require_num(
            replay.get("exit_status"),
            &format!("receipt.bug_reports[{idx}].replay_attempt.exit_status"),
        )?;
        require_str(
            replay.get("message"),
            &format!("receipt.bug_reports[{idx}].replay_attempt.message"),
        )?;
    }
    validate_checkpoint_reference(value.get("checkpoint_reference"))?;
    let raw_logs = value
        .get("raw_logs")
        .and_then(Value::as_array)
        .ok_or_else(|| EvidenceError::new("receipt.raw_logs: expected list"))?;
    for log in raw_logs {
        ensure(
            log.get("policy").and_then(Value::as_str) == Some("debug-only-excluded-from-git"),
            "raw log policy must keep logs debug-only/excluded",
        )?;
    }
    Ok(())
}

pub fn validate_markdown_receipt(data: &Value, dogfood: &Path) -> EvidenceResult<()> {
    let md_path = dogfood.join("receipt.md");
    let md = std::fs::read_to_string(&md_path).map_err(|err| {
        EvidenceError::new(format!("{}: invalid receipt.md: {err}", md_path.display()))
    })?;
    let run_id = require_str(data.get("run_id"), "receipt.run_id")?;
    ensure(
        dogfood.to_string_lossy().contains(run_id),
        "receipt.md path does not match receipt run_id context",
    )?;
    ensure(
        md.contains(&data["assertion_coverage"]["registered"].to_string()),
        "receipt.md missing assertion count",
    )?;
    for bug in data["bug_reports"].as_array().unwrap_or(&Vec::new()) {
        ensure(
            md.contains(&bug["assertion_id"].to_string()),
            "receipt.md missing bug assertion id",
        )?;
        let message = bug["replay_attempt"]["message"].as_str().unwrap_or("");
        ensure(md.contains(message), "receipt.md missing replay outcome")?;
        let context = bug["replay_context"].as_str().unwrap_or("");
        ensure(md.contains(context), "receipt.md missing replay context")?;
    }
    Ok(())
}

fn load_json(path: &Path) -> EvidenceResult<Value> {
    let text = crate::bounded_file::read_bounded_regular_file(path, MAX_EVIDENCE_JSON_BYTES)?;
    crate::json_preflight::preflight_json(&text, crate::json_preflight::QUALITY_REPORT_LIMITS)?;
    serde_json::from_str(&text)
        .map_err(|err| EvidenceError::new(format!("{}: invalid JSON: {err}", path.display())))
}

fn expect_invalid(
    path: &Path,
    validator: impl FnOnce(&Value) -> EvidenceResult<()>,
) -> EvidenceResult<()> {
    let value = load_json(path)?;
    if validator(&value).is_err() {
        Ok(())
    } else {
        Err(EvidenceError::new(format!(
            "negative fixture unexpectedly passed: {}",
            path.display()
        )))
    }
}

fn require_str<'a>(value: Option<&'a Value>, label: &str) -> EvidenceResult<&'a str> {
    let Some(value) = value
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    else {
        return Err(EvidenceError::new(format!(
            "{label}: expected non-empty string"
        )));
    };
    Ok(value)
}

fn require_num(value: Option<&Value>, label: &str) -> EvidenceResult<()> {
    ensure(
        value.is_some_and(Value::is_number),
        format!("{label}: expected number"),
    )
}

fn require_pos_int(value: Option<&Value>, label: &str) -> EvidenceResult<()> {
    ensure(
        value.and_then(Value::as_u64).is_some_and(|value| value > 0),
        format!("{label}: expected positive integer"),
    )
}

fn require_bool(value: Option<&Value>, label: &str) -> EvidenceResult<()> {
    ensure(
        value.is_some_and(Value::is_boolean),
        format!("{label}: expected bool"),
    )
}

fn require_status(value: Option<&Value>, label: &str) -> EvidenceResult<()> {
    let status = value.and_then(Value::as_str).unwrap_or("");
    ensure(
        STATUSES.contains(&status),
        format!("{label}: expected one of {:?}", sorted(STATUSES)),
    )
}

fn ensure(condition: bool, message: impl Into<String>) -> EvidenceResult<()> {
    if condition {
        Ok(())
    } else {
        Err(EvidenceError::new(message))
    }
}

fn is_prefixed_sha256(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_safe_snapshot_path(path: &Path) -> bool {
    !path.is_absolute()
        && path
            .components()
            .all(|component| !matches!(component, Component::ParentDir))
        && path.components().next() == Some(Component::Normal("snapshots".as_ref()))
}

fn sha256_file(path: &Path) -> EvidenceResult<String> {
    let bytes =
        std::fs::read(path).map_err(|err| EvidenceError::new(format!("I/O error: {err}")))?;
    let digest = Sha256::digest(&bytes);
    Ok(format!("sha256:{digest:x}"))
}

fn sorted<const N: usize>(values: [&'static str; N]) -> Vec<&'static str> {
    BTreeSet::from(values).into_iter().collect()
}
