// r[impl chaoscontrol.agent_investigation.candidate_validation_campaign]
//! Receipt for a fresh bounded validation run of candidate guest artifacts.
//!
//! The receipt binds the original failure, candidate artifact identities, the bounded
//! run profile, and the observed outcome. It does not prove a fix or replay fidelity.

use serde::{Deserialize, Serialize};

/// Receipt schema identifier.
pub const CANDIDATE_VALIDATION_SCHEMA: &str = "chaoscontrol.candidate_validation.v1";

/// Original failure facts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OriginalFailure {
    /// Checkpoint content identity (lower hex, `b3:` prefixed).
    pub checkpoint_digest: String,
    /// Recorded bug artifact name.
    pub artifact: String,
    /// Recorded bug id.
    pub bug_id: u64,
    /// Stored dedup key, when present.
    pub dedup_key: Option<u64>,
    /// Recorded failure tick.
    pub tick: u64,
}

/// One candidate artifact identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CandidateArtifact {
    /// Artifact kind (`kernel`, `initrd`, `disk_image`).
    pub kind: String,
    /// BLAKE3 identity with `b3:` prefix.
    pub digest: String,
    /// File size in bytes.
    pub size: u64,
}

/// Bounded fresh-run profile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationProfile {
    /// Seed used for the fresh run.
    pub seed: u64,
    /// Exploration rounds.
    pub rounds: u64,
    /// Ticks per branch.
    pub ticks: u64,
    /// vCPUs per VM.
    pub vcpus: usize,
    /// Extra guest cmdline, recording the selected fault/planted variant.
    #[serde(default)]
    pub extra_cmdline: String,
}

/// Observed fresh-run outcome.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationOutcome {
    /// Process exit code, when available.
    pub exit_code: Option<i32>,
    /// Whether the run exited successfully.
    pub completed: bool,
    /// Bugs found in the fresh run checkpoint, when one was written.
    pub bugs_found: Option<usize>,
    // r[impl chaoscontrol.agent_investigation.candidate_validation_counterexamples]
    /// BLAKE3 identity of the fresh checkpoint (retained counterexamples).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_digest: Option<String>,
}

/// Bound facts of one candidate validation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CandidateValidationBody {
    /// Schema identifier.
    pub schema: String,
    /// Original failure facts.
    pub original: OriginalFailure,
    /// Candidate artifact identities.
    pub candidate: Vec<CandidateArtifact>,
    /// Bounded profile.
    pub profile: ValidationProfile,
    /// Observed outcome.
    pub outcome: ValidationOutcome,
    /// Explicit limits.
    pub non_claims: Vec<String>,
}

/// Receipt with its canonical identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CandidateValidationReceipt {
    /// Bound facts.
    #[serde(flatten)]
    pub body: CandidateValidationBody,
    /// BLAKE3 identity of the canonical body encoding.
    pub receipt_digest: String,
}

/// Rejection for invalid receipt inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateValidationError {
    /// An artifact digest is not a `b3:` BLAKE3 value.
    InvalidDigest,
    /// The artifact list is empty.
    NoArtifacts,
    /// The official and candidate artifact identity are missing.
    EmptyOriginalArtifact,
    /// The profile exceeds the bounded run limits.
    InvalidProfile,
    /// The receipt digest does not match the body.
    DigestMismatch,
}

fn valid_digest(value: &str) -> bool {
    value.len() == 67
        && value.starts_with("b3:")
        && value[3..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn body_digest(body: &CandidateValidationBody) -> Result<String, CandidateValidationError> {
    let bytes = serde_json::to_vec(body).map_err(|_| CandidateValidationError::DigestMismatch)?;
    Ok(format!("b3:{}", blake3::hash(&bytes).to_hex()))
}

impl CandidateValidationReceipt {
    /// Bind the supplied facts and compute the canonical digest.
    pub fn build(
        original: OriginalFailure,
        candidate: Vec<CandidateArtifact>,
        profile: ValidationProfile,
        outcome: ValidationOutcome,
    ) -> Result<Self, CandidateValidationError> {
        if candidate.is_empty() {
            return Err(CandidateValidationError::NoArtifacts);
        }
        if candidate
            .iter()
            .any(|artifact| !valid_digest(&artifact.digest))
            || !valid_digest(&original.checkpoint_digest)
        {
            return Err(CandidateValidationError::InvalidDigest);
        }
        if original.artifact.trim().is_empty() {
            return Err(CandidateValidationError::EmptyOriginalArtifact);
        }
        if let Some(digest) = outcome.checkpoint_digest.as_deref() {
            if !valid_digest(digest) {
                return Err(CandidateValidationError::InvalidDigest);
            }
        }
        if profile.rounds == 0
            || profile.ticks == 0
            || profile.vcpus == 0
            || profile.vcpus > 8
            || profile.rounds > 1000
        {
            return Err(CandidateValidationError::InvalidProfile);
        }
        let body = CandidateValidationBody {
            schema: CANDIDATE_VALIDATION_SCHEMA.to_string(),
            original,
            candidate,
            profile,
            outcome,
            non_claims: vec![
                "A fresh run does not prove the candidate fixes the original failure.".to_string(),
                "Artifact identity does not prove a build or boot.".to_string(),
                "The observed bug count is bounded to this profile and seed.".to_string(),
            ],
        };
        let receipt_digest = body_digest(&body)?;
        Ok(Self {
            body,
            receipt_digest,
        })
    }

    /// Verify the receipt digest.
    pub fn verify(&self) -> Result<(), CandidateValidationError> {
        if body_digest(&self.body)? == self.receipt_digest {
            Ok(())
        } else {
            Err(CandidateValidationError::DigestMismatch)
        }
    }

    /// Canonical JSON with trailing newline.
    pub fn to_json(&self) -> Result<String, CandidateValidationError> {
        serde_json::to_string_pretty(self)
            .map(|mut json| {
                json.push('\n');
                json
            })
            .map_err(|_| CandidateValidationError::DigestMismatch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIGEST: &str = "b3:0000000000000000000000000000000000000000000000000000000000000001";

    fn receipt() -> CandidateValidationReceipt {
        CandidateValidationReceipt::build(
            OriginalFailure {
                checkpoint_digest: DIGEST.to_string(),
                artifact: "bug_0.json".to_string(),
                bug_id: 0,
                dedup_key: Some(9),
                tick: 1181,
            },
            vec![CandidateArtifact {
                kind: "kernel".to_string(),
                digest: DIGEST.to_string(),
                size: 10,
            }],
            ValidationProfile {
                seed: 1,
                rounds: 1,
                ticks: 100,
                vcpus: 2,
                extra_cmdline: String::new(),
            },
            ValidationOutcome {
                exit_code: Some(0),
                completed: true,
                bugs_found: Some(0),
                checkpoint_digest: Some(DIGEST.to_string()),
            },
        )
        .unwrap()
    }

    #[test]
    fn receipt_binds_and_verifies() {
        let receipt = receipt();
        receipt.verify().unwrap();
        let decoded: CandidateValidationReceipt =
            serde_json::from_str(&receipt.to_json().unwrap()).unwrap();
        assert_eq!(decoded, receipt);
        decoded.verify().unwrap();
        assert!(receipt.body.non_claims.len() == 3);
        assert_eq!(
            receipt.body.outcome.checkpoint_digest.as_deref(),
            Some(DIGEST)
        );
    }

    #[test]
    fn invalid_inputs_are_rejected() {
        assert_eq!(
            CandidateValidationReceipt::build(
                OriginalFailure {
                    checkpoint_digest: DIGEST.to_string(),
                    artifact: String::new(),
                    bug_id: 0,
                    dedup_key: None,
                    tick: 1,
                },
                vec![CandidateArtifact {
                    kind: "kernel".to_string(),
                    digest: DIGEST.to_string(),
                    size: 1,
                }],
                ValidationProfile {
                    seed: 0,
                    rounds: 1,
                    ticks: 1,
                    vcpus: 1,
                    extra_cmdline: String::new(),
                },
                ValidationOutcome {
                    exit_code: None,
                    completed: false,
                    bugs_found: None,
                    checkpoint_digest: None,
                },
            ),
            Err(CandidateValidationError::EmptyOriginalArtifact)
        );
        assert_eq!(
            CandidateValidationReceipt::build(
                receipt().body.original,
                Vec::new(),
                receipt().body.profile,
                receipt().body.outcome,
            ),
            Err(CandidateValidationError::NoArtifacts)
        );
        let mut bad = receipt();
        bad.body.candidate[0].digest = "sha256:x".to_string();
        assert_eq!(bad.verify(), Err(CandidateValidationError::DigestMismatch));
        assert_eq!(
            CandidateValidationReceipt::build(
                bad.body.original,
                bad.body.candidate,
                bad.body.profile,
                bad.body.outcome,
            ),
            Err(CandidateValidationError::InvalidDigest)
        );
    }
}
