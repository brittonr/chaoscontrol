use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::ser::Serialize;

use crate::profile_projection_spec::{validate_receipt_against_spec, ProjectionSpec, SPECS};
use crate::{EvidenceError, EvidenceResult};

pub(crate) const MAX_PROFILE_BYTES: u64 = 1024 * 1024;
pub(crate) const RECEIPT_SCHEMA: &str = "chaoscontrol.profile-projection-receipt.v1";
pub(crate) const EVALUATOR_IDENTITY: &str = "nickel-lang-cli nickel 1.17.0 (rev 1320a98)";
pub(crate) const NON_CLAIMS: [&str; 2] = [
    "profile conformance is pre-run intent only",
    "no KVM, guest, replay, fault-effect, completion, or evidence-acceptance claim",
];

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProjectionReceipt {
    pub schema: String,
    pub profile_id: String,
    pub source: BoundArtifact,
    pub contract: BoundArtifact,
    pub imports: Vec<BoundArtifact>,
    pub evaluator: BoundIdentity,
    pub projection: BoundArtifact,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BoundArtifact {
    pub path: String,
    pub identity: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BoundIdentity {
    pub name: String,
    pub identity: String,
}

pub fn check_profile_projections(root: &Path, write: bool) -> EvidenceResult<()> {
    let command = nickel_command()?;
    validate_evaluator(&command)?;
    for spec in SPECS {
        let projection = evaluate_profile(root, &command, spec.source.path)?;
        let output_bytes = canonical_pretty_json(&projection)?;
        let receipt = build_receipt(root, spec, &output_bytes)?;
        validate_receipt_against_spec(&receipt, spec)?;
        let receipt_bytes = pretty_json(&receipt)?;
        check_or_write(root.join(spec.projection.path), &output_bytes, write)?;
        check_or_write(root.join(spec.receipt), &receipt_bytes, write)?;
    }
    Ok(())
}

fn build_receipt(
    root: &Path,
    spec: &ProjectionSpec,
    projection: &[u8],
) -> EvidenceResult<ProjectionReceipt> {
    let source = bound_file(root, spec.source.path)?;
    let contract = bound_file(root, spec.contract.path)?;
    let imports = spec
        .imports
        .iter()
        .map(|artifact| bound_file(root, artifact.path))
        .collect::<EvidenceResult<Vec<_>>>()?;
    Ok(ProjectionReceipt {
        schema: RECEIPT_SCHEMA.to_string(),
        profile_id: spec.profile_id.to_string(),
        source,
        contract,
        imports,
        evaluator: BoundIdentity {
            name: EVALUATOR_IDENTITY.to_string(),
            identity: blake3_identity(EVALUATOR_IDENTITY.as_bytes()),
        },
        projection: BoundArtifact {
            path: spec.projection.path.to_string(),
            identity: blake3_identity(projection),
        },
        non_claims: NON_CLAIMS
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
    })
}

pub(crate) fn bound_file(root: &Path, relative: &str) -> EvidenceResult<BoundArtifact> {
    let bytes =
        crate::bounded_file::read_bounded_regular_file(&root.join(relative), MAX_PROFILE_BYTES)?;
    Ok(BoundArtifact {
        path: relative.to_string(),
        identity: blake3_identity(bytes.as_bytes()),
    })
}

pub(crate) fn canonical_pretty_json(bytes: &[u8]) -> EvidenceResult<Vec<u8>> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| EvidenceError::new(format!("invalid Nickel JSON projection: {error}")))?;
    pretty_json(&value)
}

fn pretty_json(value: &impl Serialize) -> EvidenceResult<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| EvidenceError::new(format!("profile JSON encoding failed: {error}")))?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub(crate) fn blake3_identity(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn evaluate_profile(root: &Path, command: &[String], source: &str) -> EvidenceResult<Vec<u8>> {
    let output = tempfile::NamedTempFile::new().map_err(|error| {
        EvidenceError::new(format!("profile output reservation failed: {error}"))
    })?;
    let stdout = output
        .reopen()
        .map_err(|error| EvidenceError::new(format!("profile output reopen failed: {error}")))?;
    let status = Command::new(&command[0])
        .args(&command[1..])
        .arg(root.join(source))
        .current_dir(root)
        .stdout(Stdio::from(stdout))
        .status()
        .map_err(|error| EvidenceError::new(format!("Nickel profile export failed: {error}")))?;
    if !status.success() {
        return Err(EvidenceError::new(format!(
            "Nickel profile export rejected {source}"
        )));
    }
    let text = crate::bounded_file::read_bounded_regular_file(output.path(), MAX_PROFILE_BYTES)?;
    Ok(text.into_bytes())
}

fn validate_evaluator(command: &[String]) -> EvidenceResult<()> {
    let output = tempfile::NamedTempFile::new().map_err(|error| {
        EvidenceError::new(format!("evaluator output reservation failed: {error}"))
    })?;
    let stdout = output
        .reopen()
        .map_err(|error| EvidenceError::new(format!("evaluator output reopen failed: {error}")))?;
    let status = Command::new(&command[0])
        .args(evaluator_version_args(command))
        .stdout(Stdio::from(stdout))
        .status()
        .map_err(|error| EvidenceError::new(format!("Nickel version check failed: {error}")))?;
    if !status.success() {
        return Err(EvidenceError::new(
            "Nickel evaluator version command failed",
        ));
    }
    let version = crate::bounded_file::read_bounded_regular_file(output.path(), MAX_PROFILE_BYTES)?;
    if version.trim() != EVALUATOR_IDENTITY {
        return Err(EvidenceError::new(format!(
            "Nickel evaluator identity drift: {:?}",
            version.trim()
        )));
    }
    Ok(())
}

fn evaluator_version_args(_command: &[String]) -> Vec<String> {
    vec!["--version".to_string()]
}

// r[impl chaoscontrol.nickel_toolchain.cohort]
// r[impl chaoscontrol.nickel_toolchain.boundary]
fn nickel_command() -> EvidenceResult<Vec<String>> {
    planned_nickel_command(command_exists("nickel"))
}

fn planned_nickel_command(available: bool) -> EvidenceResult<Vec<String>> {
    if !available {
        return Err(EvidenceError::new("exact Nickel evaluator is unavailable"));
    }
    Ok(vec!["nickel".to_string(), "export".to_string()])
}

fn command_exists(name: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|directory| directory.join(name).is_file())
}

fn check_or_write(path: PathBuf, expected: &[u8], write: bool) -> EvidenceResult<()> {
    if write {
        let parent = path
            .parent()
            .ok_or_else(|| EvidenceError::new("profile output has no parent"))?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)
            .map_err(|error| EvidenceError::new(format!("profile temp file failed: {error}")))?;
        std::io::Write::write_all(&mut temporary, expected)
            .map_err(|error| EvidenceError::new(format!("profile write failed: {error}")))?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|error| EvidenceError::new(format!("profile sync failed: {error}")))?;
        temporary.persist(&path).map_err(|error| {
            EvidenceError::new(format!("profile persist failed: {}", error.error))
        })?;
        std::fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| {
                EvidenceError::new(format!("profile directory sync failed: {error}"))
            })?;
        return Ok(());
    }
    let actual = crate::bounded_file::read_bounded_regular_file(&path, MAX_PROFILE_BYTES)?;
    if actual.as_bytes() != expected {
        return Err(EvidenceError::new(format!(
            "profile projection drift: {}",
            path.display()
        )));
    }
    Ok(())
}

pub fn verify_profile_projection(
    root: &Path,
    projection: &Path,
    receipt: &Path,
    expected_profile_id: &str,
) -> EvidenceResult<String> {
    crate::profile_projection_verification::verify_profile_projection(
        root,
        projection,
        receipt,
        expected_profile_id,
    )
}

#[cfg(test)]
#[path = "profile_projection_tests.rs"]
mod tests;
