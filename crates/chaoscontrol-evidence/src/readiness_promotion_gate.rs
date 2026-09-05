use serde_json::Value;

use crate::{EvidenceError, EvidenceResult};

const MANIFEST_SCOPE: &str = "bounded accepted snapshot-backed replay workload proofs";
const REQUIRED_REPLAY_CLASS: &str = "snapshot_backed_reproduced";
const BLOCKED_ASSERTION_IDENTITY_STATUS: &str = "blocked-assertion-identity";
const REQUIRED_ANTI_CLAIM_FRAGMENTS: [&str; 2] = [
    "does not prove global deterministic hypervisor correctness",
    "proves only the named workload",
];
const SUPPORTED_BOUNDED_STATUS: &str = "supported-bounded";
const REQUIRED_EXPERIMENTAL_SURFACES: [(&str, &str); 8] = [
    ("Rust workload authoring", "supported-bounded-rust-cohort"),
    ("Schedule-only replay", "gap-evidence-only"),
    ("Arbitrary guest/device determinism", "bounded-matrix-rail"),
    ("Hosted/fleet triage UI", "non-goal-current-scope"),
    (
        "Local multi-hypervisor control plane",
        "supported-bounded-local",
    ),
    (
        "Adapter-based distributed protocol simulation",
        "adapter-protocol-simulation",
    ),
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReportedWorkload {
    status: String,
    assertion_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ManifestWorkload {
    assertion_id: u64,
    has_fresh_receipt: bool,
}

pub fn validate_readiness_promotion_files(
    manifest_path: impl AsRef<std::path::Path>,
    report_path: impl AsRef<std::path::Path>,
) -> EvidenceResult<ReadinessPromotionSummary> {
    let manifest = load_manifest(manifest_path.as_ref())?;
    let report = load_report(report_path.as_ref())?;
    validate_readiness_promotion(&manifest, &report)
}

pub fn validate_readiness_promotion(
    manifest: &Value,
    report: &str,
) -> EvidenceResult<ReadinessPromotionSummary> {
    let manifest_entries = manifest_workload_entries(manifest)?;
    let reported = report_workloads(report)?;

    let missing_from_report: Vec<_> = manifest_entries
        .keys()
        .filter(|workload| !reported.contains_key(*workload))
        .cloned()
        .collect();
    require(
        missing_from_report.is_empty(),
        format!(
            "manifest entries missing from readiness report: {}",
            missing_from_report.join(", ")
        ),
    )?;

    let missing_from_manifest: Vec<_> = reported
        .keys()
        .filter(|workload| !manifest_entries.contains_key(*workload))
        .cloned()
        .collect();
    require(
        missing_from_manifest.is_empty(),
        format!(
            "readiness report workloads missing from manifest: {}",
            missing_from_manifest.join(", ")
        ),
    )?;

    for (workload, entry) in &manifest_entries {
        let row = reported
            .get(workload)
            .expect("reported workload set was checked against the manifest");
        let expected_status = if entry.has_fresh_receipt {
            SUPPORTED_BOUNDED_STATUS
        } else {
            BLOCKED_ASSERTION_IDENTITY_STATUS
        };
        require(
            row.status == expected_status,
            format!(
                "{workload}: readiness status {}, expected {expected_status} for receipt state {}",
                row.status, entry.has_fresh_receipt
            ),
        )?;
        require(
            row.assertion_id == entry.assertion_id,
            format!(
                "{workload}: readiness report assertion {} does not match manifest {}",
                row.assertion_id, entry.assertion_id
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
    require_protocol_simulation_surface(report)?;
    require_in_process_simulator_surface(report)?;
    require_local_rust_product_scope(report)?;

    Ok(ReadinessPromotionSummary {
        lines: manifest_entries
            .iter()
            .map(|(workload, entry)| {
                let status = if entry.has_fresh_receipt {
                    SUPPORTED_BOUNDED_STATUS
                } else {
                    BLOCKED_ASSERTION_IDENTITY_STATUS
                };
                format!(
                    "{workload}: status={status}, assertion={}",
                    entry.assertion_id
                )
            })
            .collect(),
    })
}

pub fn run_readiness_promotion_selftest(
    manifest_path: impl AsRef<std::path::Path>,
    report_path: impl AsRef<std::path::Path>,
) -> EvidenceResult<()> {
    let manifest = load_manifest(manifest_path.as_ref())?;
    let report = load_report(report_path.as_ref())?;

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
        "| Rust workload authoring | `supported-bounded-rust-cohort` |",
        "| Rust workload authoring | `experimental-rust-only` |",
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
        "| Local multi-hypervisor control plane | `supported-bounded-local` |",
        "| Local multi-hypervisor control plane | `active-local-gap` |",
    );
    expect_failure(
        "local multi-hypervisor missing promotion",
        &manifest,
        &missing_scheduler_surface,
        "Local multi-hypervisor control plane",
    )?;

    let hosted_control_plane_overclaim = report.replace(
        "not a hosted service, shared remote queue, cross-machine scheduler, universal fleet-scale throughput claim, or full Antithesis-style product replacement",
        "hosted service with shared remote queue and cross-machine scheduler support",
    );
    expect_failure(
        "local multi-hypervisor hosted overclaim",
        &manifest,
        &hosted_control_plane_overclaim,
        "local multi-hypervisor control-plane token",
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

    let protocol_simulation_overclaim = report.replace(
        "| Adapter-based distributed protocol simulation | `adapter-protocol-simulation` |",
        "| Adapter-based distributed protocol simulation | `supported-bounded` |",
    );
    expect_failure(
        "protocol simulation overclaim",
        &manifest,
        &protocol_simulation_overclaim,
        "Adapter-based distributed protocol simulation",
    )?;

    let protocol_simulation_missing_boundary = report.replace(
        "It does not prove VM replay, arbitrary protocol correctness, or Celld-equivalent behavior.",
        "It proves VM replay, arbitrary protocol correctness, and Celld-equivalent behavior.",
    );
    expect_failure(
        "protocol simulation boundary missing",
        &manifest,
        &protocol_simulation_missing_boundary,
        "protocol-simulation token",
    )?;

    let protocol_simulation_conflation = report.replace(
        "separate from VM snapshot replay proof and in-process simulator evidence",
        "the same evidence as VM snapshot replay proof and in-process simulator evidence",
    );
    expect_failure(
        "protocol simulation evidence conflation",
        &manifest,
        &protocol_simulation_conflation,
        "protocol-simulation token",
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

    let mut missing_receipt = manifest.clone();
    missing_receipt["proofs"][0]
        .as_object_mut()
        .expect("proof is an object")
        .remove("receipt");
    expect_failure(
        "receipt-less promotion",
        &missing_receipt,
        &report,
        "expected blocked-assertion-identity",
    )?;

    let alias_substitution = report.replacen(
        "| `raft` | `supported-bounded` | `3463273124` |",
        "| `raft` | `supported-bounded` | `3463273125` |",
        1,
    );
    expect_failure(
        "fresh alias substitution",
        &manifest,
        &alias_substitution,
        "does not match manifest",
    )?;

    let report_only = report.replacen(
        "| `raft` | `supported-bounded` | `3463273124` |",
        "| `new-service` | `supported-bounded` | `12345` | fresh |\n| `raft` | `supported-bounded` | `3463273124` |",
        1,
    );
    expect_failure(
        "report-only workload",
        &manifest,
        &report_only,
        "missing from manifest",
    )?;

    let missing_workload = report
        .lines()
        .filter(|line| !line.starts_with("| `raft` | `supported-bounded` |"))
        .collect::<Vec<_>>()
        .join("\n");
    expect_failure(
        "missing fresh workload",
        &manifest,
        &missing_workload,
        "manifest entries missing from readiness report",
    )?;

    Ok(())
}

fn load_manifest(path: &std::path::Path) -> EvidenceResult<Value> {
    let input =
        crate::bounded_file::read_bounded_regular_file(path, crate::MAX_EVIDENCE_JSON_BYTES)?;
    crate::json_preflight::preflight_json(&input, crate::json_preflight::QUALITY_REPORT_LIMITS)?;
    serde_json::from_str(&input)
        .map_err(|error| EvidenceError::new(format!("invalid JSON in {}: {error}", path.display())))
}

fn load_report(path: &std::path::Path) -> EvidenceResult<String> {
    crate::bounded_file::read_bounded_regular_file(path, crate::MAX_EVIDENCE_JSON_BYTES)
}

pub fn default_readiness_promotion_paths(
    root: impl AsRef<std::path::Path>,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let root = root.as_ref();
    (
        root.join("dogfood-results/accepted-workload-proofs.json"),
        root.join("docs/replay-readiness-status.md"),
    )
}

fn manifest_workload_entries(
    manifest: &Value,
) -> EvidenceResult<std::collections::BTreeMap<String, ManifestWorkload>> {
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

    let mut workloads = std::collections::BTreeMap::new();
    let mut assertion_ids = std::collections::BTreeSet::new();
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
        let has_fresh_receipt = proof
            .get("receipt")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty());
        workloads.insert(
            workload.to_string(),
            ManifestWorkload {
                assertion_id,
                has_fresh_receipt,
            },
        );
    }

    Ok(workloads)
}

fn report_workloads(
    report: &str,
) -> EvidenceResult<std::collections::BTreeMap<String, ReportedWorkload>> {
    let mut rows = std::collections::BTreeMap::new();
    let mut in_workload_table = false;
    for line in report.lines() {
        if line == "## Bounded replay evidence promotion status" {
            in_workload_table = true;
            continue;
        }
        if in_workload_table && line.starts_with("## ") {
            break;
        }
        if !in_workload_table {
            continue;
        }
        let columns = markdown_columns(line);
        if columns.len() < 3 || columns[0] == "Workload" || columns[0].starts_with('-') {
            continue;
        }
        require(
            columns[0].starts_with('`'),
            format!(
                "readiness workload row must use a code-formatted name: {}",
                columns[0]
            ),
        )?;
        let workload = strip_code(&columns[0]);
        let status = strip_code(&columns[1]).to_string();
        let assertion_id = strip_code(&columns[2]).parse::<u64>().map_err(|_| {
            EvidenceError::new(format!(
                "{workload}: readiness row assertion must be an integer"
            ))
        })?;
        require(
            rows.insert(
                workload.to_string(),
                ReportedWorkload {
                    status,
                    assertion_id,
                },
            )
            .is_none(),
            format!("duplicate readiness workload row: {workload}"),
        )?;
    }
    require(!rows.is_empty(), "readiness report has no workload rows")?;
    Ok(rows)
}

fn report_experimental_surfaces(report: &str) -> std::collections::BTreeMap<String, String> {
    let mut surfaces = std::collections::BTreeMap::new();
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

fn require_protocol_simulation_surface(report: &str) -> EvidenceResult<()> {
    let line = experimental_surface_line(report, "Adapter-based distributed protocol simulation")?;
    for token in [
        "`adapter-protocol-simulation`",
        "adapter-based protocol-simulation receipts",
        "bounded partition-failure replay fixture",
        "seed, schedule, config, artifacts, history, and output",
        "adapter-based protocol-simulation evidence only",
        "separate from VM snapshot replay proof and in-process simulator evidence",
        "does not prove VM replay",
        "arbitrary protocol correctness",
        "Celld-equivalent behavior",
        "negative nondeterminism and fault fixtures",
        "separate VMM and in-process evidence",
    ] {
        require(
            line.contains(token),
            format!("Adapter-based distributed protocol simulation row missing protocol-simulation token {token:?}"),
        )?;
    }
    for forbidden in [
        "`supported-bounded`",
        "is vm replay proof",
        "proves vm replay",
        "proves arbitrary protocol correctness",
        "celld-equivalent behavior achieved",
        "same evidence as vm snapshot replay proof",
    ] {
        require(
            !line.to_ascii_lowercase().contains(forbidden),
            format!("Adapter-based distributed protocol simulation row contains forbidden overclaim {forbidden:?}"),
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
        "supported baseline covers the admitted Rust cohort and durable one-machine multi-hypervisor orchestration",
        "Remaining gaps include broader workload admission, bounded determinism and fault coverage, local triage depth, and local artifact hygiene",
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

    require_local_multi_hypervisor_control_plane_surface(report)?;

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

fn require_local_multi_hypervisor_control_plane_surface(report: &str) -> EvidenceResult<()> {
    let line = experimental_surface_line(report, "Local multi-hypervisor control plane")?;
    for token in [
        "`supported-bounded-local`",
        "durable local multi-hypervisor campaign receipt",
        "KVM multi-hypervisor smoke rail",
        "worker resource budgets",
        "artifact roots/indexes",
        "queue-state transitions",
        "run receipts",
        "bug follow-up jobs",
        "local artifact retention",
        "not a hosted service",
        "shared remote queue",
        "cross-machine scheduler",
        "universal fleet-scale throughput claim",
        "full Antithesis-style product replacement",
    ] {
        require(
            line.contains(token),
            format!("Local multi-hypervisor control plane row missing local multi-hypervisor control-plane token {token:?}"),
        )?;
    }
    for forbidden in [
        "hosted service with",
        "shared remote queue support",
        "cross-machine scheduler support",
        "antithesis parity achieved",
    ] {
        require(
            !line.to_ascii_lowercase().contains(forbidden),
            format!("Local multi-hypervisor control plane row contains forbidden overclaim {forbidden:?}"),
        )?;
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
