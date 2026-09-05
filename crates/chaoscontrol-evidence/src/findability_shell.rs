use std::fs::{self};
use std::io::Write;

pub const FINDABILITY_ARTIFACT_SCHEMA_VERSION: u32 = 1;
pub const MAX_FINDABILITY_ARTIFACT_BYTES: u64 = 4 * 1_024 * 1_024;
const ARTIFACT_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.findability.round-artifact.v1\0";

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindabilityRoundArtifact {
    pub schema_version: u32,
    pub generation_id: String,
    pub artifact_blake3: String,
    pub policy: ::chaoscontrol_sim_core::findability::FindabilityPolicy,
    pub subtrees: Vec<RoundSubtree>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoundSubtree {
    pub subtree_id: String,
    pub independence_group: String,
    pub observed_time: u64,
    pub bugs: Vec<::chaoscontrol_sim_core::findability::BugInstance>,
}

pub fn bind_findability_artifact(
    generation_id: impl Into<String>,
    policy: ::chaoscontrol_sim_core::findability::FindabilityPolicy,
    mut subtrees: Vec<RoundSubtree>,
) -> crate::EvidenceResult<FindabilityRoundArtifact> {
    subtrees.sort_by(|left, right| left.subtree_id.cmp(&right.subtree_id));
    for subtree in &mut subtrees {
        subtree.bugs.sort();
    }
    let mut artifact = FindabilityRoundArtifact {
        schema_version: FINDABILITY_ARTIFACT_SCHEMA_VERSION,
        generation_id: generation_id.into(),
        artifact_blake3: String::new(),
        policy,
        subtrees,
    };
    artifact.artifact_blake3 = artifact_identity(&artifact)?;
    validate_findability_artifact(&artifact)?;
    Ok(artifact)
}

pub fn validate_findability_artifact(
    artifact: &FindabilityRoundArtifact,
) -> crate::EvidenceResult<()> {
    require(
        artifact.schema_version == FINDABILITY_ARTIFACT_SCHEMA_VERSION,
        "unsupported findability artifact schema",
    )?;
    require(
        !artifact.generation_id.is_empty(),
        "findability generation_id must be non-empty",
    )?;
    let mut canonical = artifact.subtrees.clone();
    canonical.sort_by(|left, right| left.subtree_id.cmp(&right.subtree_id));
    for subtree in &mut canonical {
        subtree.bugs.sort();
    }
    require(
        canonical == artifact.subtrees,
        "findability subtrees and bug instances must use canonical order",
    )?;
    let expected = artifact_identity(artifact)?;
    require(
        artifact.artifact_blake3 == expected,
        "findability artifact BLAKE3 identity drifted",
    )?;
    let observations = artifact_observations(artifact)?;
    let report =
        ::chaoscontrol_sim_core::findability::fit_findability(&observations, &artifact.policy)
            .map_err(|error| crate::EvidenceError::new(error.to_string()))?;
    ::chaoscontrol_sim_core::findability::validate_report(&report, &observations, &artifact.policy)
        .map_err(|error| crate::EvidenceError::new(error.to_string()))
}

pub fn read_findability_artifact_path(
    path: impl AsRef<std::path::Path>,
) -> crate::EvidenceResult<FindabilityRoundArtifact> {
    let path = path.as_ref();
    let bytes =
        crate::bounded_file::read_bounded_regular_bytes(path, MAX_FINDABILITY_ARTIFACT_BYTES)?;
    let artifact = serde_json::from_slice::<FindabilityRoundArtifact>(&bytes).map_err(|error| {
        crate::EvidenceError::new(format!(
            "{}: findability JSON is not a closed typed artifact: {error}",
            path.display()
        ))
    })?;
    validate_findability_artifact(&artifact)?;
    Ok(artifact)
}

pub fn check_findability_artifact_path(
    path: impl AsRef<std::path::Path>,
) -> crate::EvidenceResult<::chaoscontrol_sim_core::findability::FindabilityReport> {
    let artifact = read_findability_artifact_path(path)?;
    let observations = artifact_observations(&artifact)?;
    let report =
        ::chaoscontrol_sim_core::findability::fit_findability(&observations, &artifact.policy)
            .map_err(|error| crate::EvidenceError::new(error.to_string()))?;
    ::chaoscontrol_sim_core::findability::validate_report(&report, &observations, &artifact.policy)
        .map_err(|error| crate::EvidenceError::new(error.to_string()))?;
    Ok(report)
}

pub fn write_findability_report_path(
    artifact_path: impl AsRef<std::path::Path>,
    report_path: impl AsRef<std::path::Path>,
) -> crate::EvidenceResult<()> {
    let report = check_findability_artifact_path(artifact_path)?;
    let mut bytes = serde_json::to_vec_pretty(&report)?;
    bytes.push(b'\n');
    let report_path = report_path.as_ref();
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(report_path)
        .map_err(|error| {
            crate::EvidenceError::new(format!("{}: {error}", report_path.display()))
        })?;
    if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
        drop(file);
        let cleanup = fs::remove_file(report_path);
        return Err(crate::EvidenceError::new(match cleanup {
            Ok(()) => format!("{}: {error}", report_path.display()),
            Err(cleanup_error) => format!(
                "{}: {error}; failed to remove partial report: {cleanup_error}",
                report_path.display()
            ),
        }));
    }
    Ok(())
}

fn artifact_observations(
    artifact: &FindabilityRoundArtifact,
) -> crate::EvidenceResult<Vec<chaoscontrol_sim_core::findability::AssembledObservation>> {
    let subtrees = artifact
        .subtrees
        .iter()
        .map(
            |subtree| ::chaoscontrol_sim_core::findability::SubtreeObservation {
                generation_id: artifact.generation_id.clone(),
                subtree_id: subtree.subtree_id.clone(),
                independence_group: subtree.independence_group.clone(),
                observed_time: subtree.observed_time,
                source_blake3: artifact.artifact_blake3.clone(),
                bugs: subtree.bugs.clone(),
            },
        )
        .collect::<Vec<_>>();
    ::chaoscontrol_sim_core::findability::assemble_observations(&subtrees)
        .map_err(|error| crate::EvidenceError::new(error.to_string()))
}

fn artifact_identity(artifact: &FindabilityRoundArtifact) -> crate::EvidenceResult<String> {
    #[derive(serde::Serialize)]
    struct Material<'a> {
        schema_version: u32,
        generation_id: &'a str,
        policy: &'a ::chaoscontrol_sim_core::findability::FindabilityPolicy,
        subtrees: &'a [RoundSubtree],
    }
    let material = Material {
        schema_version: artifact.schema_version,
        generation_id: &artifact.generation_id,
        policy: &artifact.policy,
        subtrees: &artifact.subtrees,
    };
    let bytes = serde_json::to_vec(&material)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(ARTIFACT_IDENTITY_DOMAIN);
    let length = u64::try_from(bytes.len())
        .map_err(|_| crate::EvidenceError::new("findability artifact length exceeds u64"))?;
    hasher.update(&length.to_le_bytes());
    hasher.update(&bytes);
    Ok(format!("blake3:{}", hasher.finalize().to_hex()))
}

fn require(condition: bool, message: &'static str) -> crate::EvidenceResult<()> {
    if condition {
        Ok(())
    } else {
        Err(crate::EvidenceError::new(message))
    }
}
