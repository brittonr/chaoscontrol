use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use chaoscontrol_snapshot_descriptor as snapshot_core;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::snapshot_descriptor::fixture::example_descriptor;
use crate::{EvidenceError, EvidenceResult};

const CONTRACT_DIR: &str = "contracts/evidence";
const CONTRACT_FILE: &str = "snapshot-descriptor.ncl";
const EXAMPLE_FILE: &str = "examples/snapshot-descriptor.ncl";
const INVALID_SCHEMA_FILE: &str = "fixtures/invalid/snapshot-descriptor.schema.invalid.ncl";
const VALID_FIXTURE_FILE: &str = "fixtures/valid/snapshot-descriptor.valid.json";
const SCHEMA_FILE: &str = "schema/snapshot-descriptor-v1.schema.json";
const FRESHNESS_FILE: &str = "snapshot-descriptor.freshness.json";
const FRESHNESS_SCHEMA: &str = "chaoscontrol-snapshot-descriptor-freshness-v1";
const SOURCE_MODEL_FILE: &str = "crates/chaoscontrol-snapshot-descriptor/src/model.rs";
const SOURCE_OBSERVATIONS_FILE: &str =
    "crates/chaoscontrol-snapshot-descriptor/src/observations.rs";

const DESCRIPTOR_FIELDS: [&str; 10] = [
    "architecture",
    "completeness_profile",
    "descriptor_version",
    "guest_artifacts",
    "payload",
    "runtime",
    "schema",
    "state_owners",
    "state_schema_version",
    "topology",
];

const FRESHNESS_INPUTS: [&str; 6] = [
    SOURCE_MODEL_FILE,
    SOURCE_OBSERVATIONS_FILE,
    "contracts/evidence/snapshot-descriptor.ncl",
    "contracts/evidence/examples/snapshot-descriptor.ncl",
    "contracts/evidence/fixtures/valid/snapshot-descriptor.valid.json",
    "contracts/evidence/schema/snapshot-descriptor-v1.schema.json",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FreshnessManifest {
    schema: String,
    files: Vec<FreshnessFile>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FreshnessFile {
    path: String,
    blake3: String,
}

// r[impl chaoscontrol.snapshot_descriptor.projection]
pub fn check_snapshot_descriptor_contracts(
    root: impl AsRef<Path>,
    write: bool,
) -> EvidenceResult<()> {
    let root = root.as_ref();
    let contract_root = root.join(CONTRACT_DIR);
    let fixture_path = contract_root.join(VALID_FIXTURE_FILE);
    let expected = example_descriptor()?;
    if write {
        write_json(&fixture_path, &expected)?;
    }
    let fixture_bytes = std::fs::read(&fixture_path)?;
    let fixture: snapshot_core::SnapshotDescriptor = serde_json::from_slice(&fixture_bytes)?;
    snapshot_core::validate_descriptor(&fixture)
        .map_err(|error| EvidenceError::new(error.to_string()))?;
    if fixture != expected {
        return Err(EvidenceError::new(
            "snapshot descriptor fixture is stale relative to the Rust owner",
        ));
    }
    validate_schema_snapshot(&contract_root.join(SCHEMA_FILE))?;
    validate_nickel_contract(root, &contract_root, &fixture)?;
    let freshness = build_freshness(root)?;
    let freshness_path = contract_root.join(FRESHNESS_FILE);
    if write {
        write_json(&freshness_path, &freshness)?;
    }
    let committed: FreshnessManifest = serde_json::from_slice(&std::fs::read(&freshness_path)?)?;
    if committed != freshness {
        return Err(EvidenceError::new(
            "snapshot descriptor contract freshness manifest is stale",
        ));
    }
    Ok(())
}

fn validate_schema_snapshot(path: &Path) -> EvidenceResult<()> {
    let schema: Value = serde_json::from_slice(&std::fs::read(path)?)?;
    if schema.get("$id").and_then(Value::as_str) != Some(snapshot_core::DESCRIPTOR_SCHEMA) {
        return Err(EvidenceError::new("snapshot descriptor schema ID is stale"));
    }
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| EvidenceError::new("snapshot descriptor schema lacks properties"))?;
    let property_names = properties
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let expected_names = DESCRIPTOR_FIELDS.into_iter().collect::<BTreeSet<_>>();
    if property_names != expected_names {
        return Err(EvidenceError::new(
            "snapshot descriptor schema properties differ from the Rust owner",
        ));
    }
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .ok_or_else(|| EvidenceError::new("snapshot descriptor schema lacks required fields"))?;
    let required_names = required
        .iter()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    if required_names != expected_names {
        return Err(EvidenceError::new(
            "snapshot descriptor schema required fields differ from the Rust owner",
        ));
    }
    Ok(())
}

fn validate_nickel_contract(
    root: &Path,
    contract_root: &Path,
    fixture: &snapshot_core::SnapshotDescriptor,
) -> EvidenceResult<()> {
    let nickel = nickel_command()?;
    let output = Command::new(&nickel[0])
        .args(&nickel[1..])
        .args(["export", "--format", "json"])
        .arg(contract_root.join(EXAMPLE_FILE))
        .current_dir(root)
        .output()
        .map_err(|error| EvidenceError::new(format!("cannot run Nickel: {error}")))?;
    if !output.status.success() {
        return Err(EvidenceError::new(format!(
            "snapshot descriptor Nickel example failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    let exported: snapshot_core::SnapshotDescriptor = serde_json::from_slice(&output.stdout)?;
    if &exported != fixture {
        return Err(EvidenceError::new(
            "snapshot descriptor Nickel projection differs from the Rust fixture",
        ));
    }
    let invalid_status = Command::new(&nickel[0])
        .args(&nickel[1..])
        .arg("export")
        .arg(contract_root.join(INVALID_SCHEMA_FILE))
        .current_dir(root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| {
            EvidenceError::new(format!("cannot run negative Nickel fixture: {error}"))
        })?;
    if invalid_status.success() {
        return Err(EvidenceError::new(
            "invalid snapshot descriptor Nickel fixture unexpectedly passed",
        ));
    }
    Ok(())
}

fn build_freshness(root: &Path) -> EvidenceResult<FreshnessManifest> {
    let mut files = FRESHNESS_INPUTS
        .iter()
        .map(|relative| {
            let bytes = std::fs::read(root.join(relative))?;
            Ok(FreshnessFile {
                path: relative.to_string(),
                blake3: blake3::hash(&bytes).to_hex().to_string(),
            })
        })
        .collect::<EvidenceResult<Vec<_>>>()?;
    files.sort();
    Ok(FreshnessManifest {
        schema: FRESHNESS_SCHEMA.to_string(),
        files,
    })
}

fn nickel_command() -> EvidenceResult<Vec<String>> {
    let status = Command::new("nickel")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if status.is_ok_and(|value| value.success()) {
        return Ok(vec!["nickel".to_string()]);
    }
    Err(EvidenceError::new(
        "nickel is required for snapshot descriptor contract checks",
    ))
}

fn write_json(path: impl AsRef<Path>, value: &impl Serialize) -> EvidenceResult<()> {
    let path = path.as_ref();
    let parent = path
        .parent()
        .ok_or_else(|| EvidenceError::new("generated contract path lacks a parent"))?;
    std::fs::create_dir_all(parent)?;
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    std::fs::write(path, bytes)?;
    Ok(())
}

pub fn contract_paths(root: impl AsRef<Path>) -> Vec<PathBuf> {
    let root = root.as_ref().join(CONTRACT_DIR);
    [
        CONTRACT_FILE,
        EXAMPLE_FILE,
        INVALID_SCHEMA_FILE,
        VALID_FIXTURE_FILE,
        SCHEMA_FILE,
        FRESHNESS_FILE,
    ]
    .iter()
    .map(|path| root.join(path))
    .collect()
}
