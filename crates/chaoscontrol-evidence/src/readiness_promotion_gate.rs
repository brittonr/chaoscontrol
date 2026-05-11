use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{EvidenceError, EvidenceResult};

const MANIFEST_SCOPE: &str = "bounded accepted snapshot-backed replay workload proofs";
const REQUIRED_REPLAY_CLASS: &str = "snapshot_backed_reproduced";
const REQUIRED_ANTI_CLAIM_FRAGMENTS: [&str; 2] = [
    "does not prove global deterministic hypervisor correctness",
    "proves only the named workload",
];
const REQUIRED_EXPERIMENTAL_SURFACES: [(&str, &str); 7] = [
    ("Rust workload authoring", "experimental-rust-only"),
    ("Schedule-only replay", "gap-evidence-only"),
    ("Arbitrary guest/device determinism", "bounded-matrix-rail"),
    ("Hosted/fleet triage UI", "non-goal-current-scope"),
    ("Local multi-hypervisor control plane", "active-local-gap"),
    (
        "FoundationDB-style in-process deterministic simulator",
        "adapter-simulator-receipt",
    ),
    (
        "Full Antithesis-style product replacement",
        "non-goal-current-scope",
    ),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadinessPromotionSummary {
    pub lines: Vec<String>,
}

pub fn validate_readiness_promotion_files(
    manifest_path: impl AsRef<Path>,
    report_path: impl AsRef<Path>,
) -> EvidenceResult<ReadinessPromotionSummary> {
    let manifest_text = fs::read_to_string(manifest_path.as_ref()).map_err(|err| {
        EvidenceError::new(format!(
            "missing file: {} ({err})",
            manifest_path.as_ref().display()
        ))
    })?;
    let manifest: Value = serde_json::from_str(&manifest_text).map_err(|err| {
        EvidenceError::new(format!(
            "invalid JSON in {}: {err}",
            manifest_path.as_ref().display()
        ))
    })?;
    let report = fs::read_to_string(report_path.as_ref()).map_err(|err| {
        EvidenceError::new(format!(
            "missing file: {} ({err})",
            report_path.as_ref().display()
        ))
    })?;
    validate_readiness_promotion(&manifest, &report)
}

pub fn validate_readiness_promotion(
    manifest: &Value,
    report: &str,
) -> EvidenceResult<ReadinessPromotionSummary> {
    let proofs = manifest_proofs(manifest)?;
    let supported = report_supported_workloads(report)?;

    let missing_from_report: Vec<_> = proofs
        .keys()
        .filter(|workload| !supported.contains_key(*workload))
        .cloned()
        .collect();
    require(
        missing_from_report.is_empty(),
        format!(
            "accepted manifest proofs missing from readiness report: {}",
            missing_from_report.join(", ")
        ),
    )?;

    let unsupported_in_report: Vec<_> = supported
        .keys()
        .filter(|workload| !proofs.contains_key(*workload))
        .cloned()
        .collect();
    require(
        unsupported_in_report.is_empty(),
        format!(
            "readiness report promotes workloads missing from manifest: {}",
            unsupported_in_report.join(", ")
        ),
    )?;

    for (workload, assertion_id) in &proofs {
        let report_assertion = supported
            .get(workload)
            .expect("supported set was already checked against proofs");
        require(
            report_assertion == assertion_id,
            format!(
                "{workload}: readiness report assertion {report_assertion} does not match manifest {assertion_id}"
            ),
        )?;
    }

    let surfaces = report_experimental_surfaces(report);
    for (surface, expected_status) in REQUIRED_EXPERIMENTAL_SURFACES {
        let actual_status = surfaces.get(surface).map(String::as_str);
        require(
            actual_status == Some(expected_status),
            format!(
                "experimental surface {surface:?} status {actual_status:?}, expected {expected_status:?}"
            ),
        )?;
    }
    require_bounded_matrix_surface(report)?;
    require_in_process_simulator_surface(report)?;
    require_local_rust_product_scope(report)?;

    Ok(ReadinessPromotionSummary {
        lines: proofs
            .iter()
            .map(|(workload, assertion_id)| format!("{workload}: assertion={assertion_id}"))
            .collect(),
    })
}

pub fn run_readiness_promotion_selftest(
    manifest_path: impl AsRef<Path>,
    report_path: impl AsRef<Path>,
) -> EvidenceResult<()> {
    let manifest_text = fs::read_to_string(manifest_path.as_ref()).map_err(|err| {
        EvidenceError::new(format!(
            "missing file: {} ({err})",
            manifest_path.as_ref().display()
        ))
    })?;
    let manifest: Value = serde_json::from_str(&manifest_text).map_err(|err| {
        EvidenceError::new(format!(
            "invalid JSON in {}: {err}",
            manifest_path.as_ref().display()
        ))
    })?;
    let report = fs::read_to_string(report_path.as_ref()).map_err(|err| {
        EvidenceError::new(format!(
            "missing file: {} ({err})",
            report_path.as_ref().display()
        ))
    })?;

    validate_readiness_promotion(&manifest, &report)?;

    let mut missing_claim = manifest.clone();
    *missing_claim
        .as_object_mut()
        .expect("committed manifest is an object")
        .get_mut("anti_claims")
        .expect("committed manifest has anti_claims") =
        Value::Array(vec![Value::String("bounded only".to_string())]);
    expect_failure(
        "missing anti-claim",
        &missing_claim,
        &report,
        "anti_claims missing fragment",
    )?;

    let mut duplicate_assertion = manifest.clone();
    let proofs = duplicate_assertion
        .as_object_mut()
        .expect("committed manifest is an object")
        .get_mut("proofs")
        .and_then(Value::as_array_mut)
        .expect("committed manifest has proofs");
    let first_assertion = proofs[0]
        .get("assertion_id")
        .expect("committed proof has assertion_id")
        .clone();
    proofs[1]
        .as_object_mut()
        .expect("committed proof is object")
        .insert("assertion_id".to_string(), first_assertion);
    expect_failure(
        "duplicate assertion",
        &duplicate_assertion,
        &report,
        "duplicate assertion_id",
    )?;

    let missing_fresh_surface = report.replace(
        "| Rust workload authoring | `experimental-rust-only` |",
        "| Rust workload authoring | `supported-bounded` |",
    );
    expect_failure(
        "fresh Rust workload overclaim",
        &manifest,
        &missing_fresh_surface,
        "Rust workload authoring",
    )?;

    let missing_hosted_fleet_surface = report.replace(
        "| Hosted/fleet triage UI | `non-goal-current-scope` |",
        "| Hosted/fleet triage UI | `supported-bounded` |",
    );
    expect_failure(
        "hosted fleet triage overclaim",
        &manifest,
        &missing_hosted_fleet_surface,
        "Hosted/fleet triage UI",
    )?;

    let missing_scheduler_surface = report.replace(
        "| Local multi-hypervisor control plane | `active-local-gap` |",
        "| Local multi-hypervisor control plane | `supported-bounded` |",
    );
    expect_failure(
        "local multi-hypervisor overclaim",
        &manifest,
        &missing_scheduler_surface,
        "Local multi-hypervisor control plane",
    )?;

    let hosted_scope_overclaim = report.replace(
        "Hosted services, cross-machine fleet scheduling, and non-Rust SDKs are out of current product scope",
        "Hosted services, cross-machine fleet scheduling, and non-Rust SDKs are current missing features",
    );
    expect_failure(
        "current scope hosted/multi-language overclaim",
        &manifest,
        &hosted_scope_overclaim,
        "current product scope token",
    )?;

    let missing_matrix_tokens = report.replace(".#vm-determinism-matrix", ".#vm-determinism-drift");
    expect_failure(
        "matrix rail evidence missing",
        &manifest,
        &missing_matrix_tokens,
        "bounded matrix token",
    )?;

    let arbitrary_determinism_overclaim = report.replace(
        "| Arbitrary guest/device determinism | `bounded-matrix-rail` |",
        "| Arbitrary guest/device determinism | `supported-bounded` |",
    );
    expect_failure(
        "arbitrary determinism overclaim",
        &manifest,
        &arbitrary_determinism_overclaim,
        "Arbitrary guest/device determinism",
    )?;

    let simulator_overclaim = report.replace(
        "| FoundationDB-style in-process deterministic simulator | `adapter-simulator-receipt` |",
        "| FoundationDB-style in-process deterministic simulator | `supported-bounded` |",
    );
    expect_failure(
        "in-process simulator overclaim",
        &manifest,
        &simulator_overclaim,
        "FoundationDB-style in-process deterministic simulator",
    )?;

    let simulator_missing_boundary =
        report.replace("not full FoundationDB parity", "FoundationDB parity");
    expect_failure(
        "in-process simulator boundary missing",
        &manifest,
        &simulator_missing_boundary,
        "in-process simulator token",
    )?;

    let report_only = report.replacen(
        "| `raft` | `supported-bounded` | `1806003755` |",
        "| `new-service` | `supported-bounded` | `12345` |\n| `raft` | `supported-bounded` | `1806003755` |",
        1,
    );
    expect_failure(
        "report-only promotion",
        &manifest,
        &report_only,
        "missing from manifest",
    )?;

    Ok(())
}

pub fn default_readiness_promotion_paths(root: impl AsRef<Path>) -> (PathBuf, PathBuf) {
    let root = root.as_ref();
    (
        root.join("dogfood-results/accepted-workload-proofs.json"),
        root.join("docs/replay-readiness-status.md"),
    )
}

fn manifest_proofs(manifest: &Value) -> EvidenceResult<BTreeMap<String, u64>> {
    let object = manifest
        .as_object()
        .ok_or_else(|| EvidenceError::new("manifest must be a JSON object".to_string()))?;
    require(
        object.get("schema_version").and_then(Value::as_u64) == Some(1),
        "manifest schema_version must be 1",
    )?;
    require(
        object.get("scope").and_then(Value::as_str) == Some(MANIFEST_SCOPE),
        "manifest scope must remain bounded accepted snapshot-backed replay workload proofs",
    )?;
    require(
        object.get("required_replay_class").and_then(Value::as_str) == Some(REQUIRED_REPLAY_CLASS),
        "manifest required_replay_class must remain snapshot_backed_reproduced",
    )?;

    let anti_claims = object
        .get("anti_claims")
        .and_then(Value::as_array)
        .ok_or_else(|| EvidenceError::new("manifest anti_claims must be a list".to_string()))?;
    let anti_claim_text = anti_claims
        .iter()
        .map(|item| item.as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    for fragment in REQUIRED_ANTI_CLAIM_FRAGMENTS {
        require(
            anti_claim_text.contains(fragment),
            format!("manifest anti_claims missing fragment: {fragment}"),
        )?;
    }

    let proofs = object
        .get("proofs")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            EvidenceError::new("manifest proofs must be a non-empty list".to_string())
        })?;
    require(
        !proofs.is_empty(),
        "manifest proofs must be a non-empty list",
    )?;

    let mut workloads = BTreeMap::new();
    let mut assertion_ids = BTreeSet::new();
    for (index, proof) in proofs.iter().enumerate() {
        let proof = proof
            .as_object()
            .ok_or_else(|| EvidenceError::new(format!("proof[{index}] must be an object")))?;
        let workload = proof.get("workload").and_then(Value::as_str).unwrap_or("");
        require(
            !workload.is_empty(),
            format!("proof[{index}].workload must be non-empty"),
        )?;
        require(
            !workloads.contains_key(workload),
            format!("duplicate workload proof: {workload}"),
        )?;
        let assertion_id = proof
            .get("assertion_id")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                EvidenceError::new(format!("{workload}: assertion_id must be an integer"))
            })?;
        require(
            assertion_ids.insert(assertion_id),
            format!("duplicate assertion_id: {assertion_id}"),
        )?;
        for field in [
            "evidence_dir",
            "summary",
            "bug",
            "verdict",
            "snapshot",
            "notes",
        ] {
            require(
                proof
                    .get(field)
                    .and_then(Value::as_str)
                    .is_some_and(|value| !value.is_empty()),
                format!("{workload}: proof.{field} must be non-empty"),
            )?;
        }
        workloads.insert(workload.to_string(), assertion_id);
    }

    Ok(workloads)
}

fn report_supported_workloads(report: &str) -> EvidenceResult<BTreeMap<String, u64>> {
    let mut rows = BTreeMap::new();
    for line in report.lines() {
        let columns = markdown_columns(line);
        if columns.len() < 3 || columns[0] == "Workload" || columns[0].starts_with('-') {
            continue;
        }
        let workload = strip_code(&columns[0]);
        let status = strip_code(&columns[1]);
        if status != "supported-bounded" || !columns[0].starts_with('`') {
            continue;
        }
        let assertion_id = strip_code(&columns[2]).parse::<u64>().map_err(|_| {
            EvidenceError::new(format!(
                "{workload}: readiness row assertion must be an integer"
            ))
        })?;
        require(
            rows.insert(workload.to_string(), assertion_id).is_none(),
            format!("duplicate supported readiness row: {workload}"),
        )?;
    }
    require(
        !rows.is_empty(),
        "readiness report has no supported-bounded workload rows",
    )?;
    Ok(rows)
}

fn report_experimental_surfaces(report: &str) -> BTreeMap<String, String> {
    let mut surfaces = BTreeMap::new();
    let mut in_experimental = false;
    for line in report.lines() {
        if line == "## Experimental or unproven surfaces" {
            in_experimental = true;
            continue;
        }
        if in_experimental && line.starts_with("## ") {
            break;
        }
        if !in_experimental {
            continue;
        }
        let columns = markdown_columns(line);
        if columns.len() >= 2 && !columns[0].starts_with('-') && columns[0] != "Surface" {
            surfaces.insert(columns[0].to_string(), strip_code(&columns[1]).to_string());
        }
    }
    surfaces
}

fn require_bounded_matrix_surface(report: &str) -> EvidenceResult<()> {
    let line = experimental_surface_line(report, "Arbitrary guest/device determinism")?;
    for token in [
        "`bounded-matrix-rail`",
        ".#vm-determinism-matrix",
        "`matrix-receipt.json`",
        "matrix-scoped evidence only",
        "not a universal hypervisor/device/timing determinism proof",
        "negative drift evidence",
    ] {
        require(
            line.contains(token),
            format!(
                "Arbitrary guest/device determinism row missing bounded matrix token {token:?}"
            ),
        )?;
    }
    for forbidden in [
        "`supported-bounded`",
        "proves universal determinism",
        "claims universal determinism",
        "proves arbitrary determinism",
        "all guests",
        "all devices",
    ] {
        require(
            !line.to_ascii_lowercase().contains(forbidden),
            format!(
                "Arbitrary guest/device determinism row contains forbidden overclaim {forbidden:?}"
            ),
        )?;
    }
    Ok(())
}

fn require_in_process_simulator_surface(report: &str) -> EvidenceResult<()> {
    let line = experimental_surface_line(
        report,
        "FoundationDB-style in-process deterministic simulator",
    )?;
    for token in [
        "`adapter-simulator-receipt`",
        "`in-process-simulator-receipt`",
        "deterministic scheduler",
        "virtual clock",
        "simulated network/disk hooks",
        "sim-vm bridge metadata",
        "simulator-local vs vm-snapshot-replay evidence classes",
        "not VM replay proof",
        "not arbitrary binary support",
        "not full FoundationDB parity",
        "separate VMM replay evidence",
    ] {
        require(
            line.contains(token),
            format!("FoundationDB-style in-process deterministic simulator row missing in-process simulator token {token:?}"),
        )?;
    }
    for forbidden in [
        "`supported-bounded`",
        "is vm replay proof",
        "proves vm replay",
        "supports arbitrary binaries",
        "foundationdb parity achieved",
    ] {
        require(
            !line.to_ascii_lowercase().contains(forbidden),
            format!("FoundationDB-style in-process deterministic simulator row contains forbidden overclaim {forbidden:?}"),
        )?;
    }
    Ok(())
}

fn require_local_rust_product_scope(report: &str) -> EvidenceResult<()> {
    let summary = report;
    for token in [
        "Current product target: Rust-only workload support on one machine with multiple local ChaosControl hypervisors",
        "remaining product gaps are local multi-hypervisor control-plane depth, Rust workload authoring/onboarding, bounded determinism/fault coverage, local triage, and local artifact hygiene",
        "Hosted services, cross-machine fleet scheduling, and non-Rust SDKs are out of current product scope",
        "Rust workload authoring",
        "Local multi-hypervisor control plane",
        "`non-goal-current-scope`",
    ] {
        require(
            summary.contains(token),
            format!("readiness report missing current product scope token {token:?}"),
        )?;
    }

    for line in [
        experimental_surface_line(report, "Hosted/fleet triage UI")?,
        experimental_surface_line(report, "Full Antithesis-style product replacement")?,
    ] {
        let lower = line.to_ascii_lowercase();
        for forbidden in [
            "`supported-bounded`",
            "active missing feature",
            "current product gap",
        ] {
            require(
                !lower.contains(forbidden),
                format!(
                    "out-of-scope row contains forbidden current-scope overclaim {forbidden:?}"
                ),
            )?;
        }
    }

    Ok(())
}

fn experimental_surface_line<'a>(report: &'a str, surface: &str) -> EvidenceResult<&'a str> {
    report
        .lines()
        .find(|line| line.starts_with(&format!("| {surface} |")))
        .ok_or_else(|| EvidenceError::new(format!("missing experimental surface row: {surface}")))
}

fn markdown_columns(line: &str) -> Vec<String> {
    if !line.starts_with('|') {
        return Vec::new();
    }
    line.trim_matches('|')
        .split('|')
        .map(|part| part.trim().to_string())
        .collect()
}

fn strip_code(value: &str) -> &str {
    value.trim().trim_matches('`')
}

fn expect_failure(name: &str, manifest: &Value, report: &str, needle: &str) -> EvidenceResult<()> {
    match validate_readiness_promotion(manifest, report) {
        Ok(_) => Err(EvidenceError::new(format!("{name}: unexpectedly passed"))),
        Err(err) if err.to_string().contains(needle) => Ok(()),
        Err(err) => Err(EvidenceError::new(format!(
            "{name}: expected {needle:?}, got {err}"
        ))),
    }
}

fn require(condition: bool, message: impl Into<String>) -> EvidenceResult<()> {
    if condition {
        Ok(())
    } else {
        Err(EvidenceError::new(message.into()))
    }
}
