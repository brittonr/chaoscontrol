//! Reviewer receipt over a failure, its replay, branch, and candidate runs.
//!
//! Binds the original recorded failure, the replay verdict, the disposable
//! branch identity, and positive/negative candidate-validation receipts under
//! one BLAKE3 identity. It summarizes observed bounded facts; it does not prove
//! a fix, replay fidelity, or release eligibility.

use crate::candidate_validation::{CandidateValidationReceipt, OriginalFailure};
use serde::{Deserialize, Serialize};

/// Reviewer receipt schema identifier.
pub const CANDIDATE_REVIEW_SCHEMA: &str = "chaoscontrol.candidate_review.v1";

/// Replay facts extracted from a verdict document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayFacts {
    /// BLAKE3 identity of the verdict document bytes.
    pub verdict_digest: String,
    /// Recorded replay class.
    pub replay_class: String,
    /// Whether the failure reproduced.
    pub reproduced: bool,
    /// Recorded snapshot status.
    pub snapshot_status: String,
}

/// One candidate campaign summarized for review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateRow {
    /// Positive or negative role.
    pub role: String,
    /// BLAKE3 identity of the candidate-validation receipt bytes.
    pub receipt_digest: String,
    /// Candidate artifact identities from the receipt.
    pub candidate_digests: Vec<String>,
    /// Recorded extra cmdline.
    pub extra_cmdline: String,
    /// Whether the fresh run completed with a checkpoint.
    pub completed: bool,
    /// Bugs found in the fresh checkpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bugs_found: Option<usize>,
    /// Fresh checkpoint identity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_digest: Option<String>,
}

/// Reviewer verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewVerdict {
    /// Replay reproduced, positive found findings, negative found none.
    Ready,
    /// Required evidence is missing or incomplete.
    Incomplete,
    /// Evidence contradicts the expected candidate roles.
    Fail,
}

/// Reviewer receipt with its canonical identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateReviewReceipt {
    /// Schema identifier.
    pub schema: String,
    /// Original recorded failure.
    pub original: OriginalFailure,
    /// Disposable branch identity from the probe.
    pub branch_id: String,
    /// Replay verdict facts.
    pub replay: ReplayFacts,
    /// Positive candidate row.
    pub positive: CandidateRow,
    /// Negative candidate row.
    pub negative: CandidateRow,
    /// Reviewer verdict.
    pub verdict: ReviewVerdict,
    /// BLAKE3 identity of the canonical body (excluding this field).
    pub review_digest: String,
    /// Explicit boundaries.
    pub non_claims: Vec<String>,
}

/// Reviewer receipt failure.
#[derive(Debug)]
pub enum ReviewError {
    /// An input document is malformed or has the wrong schema.
    Input(String),
    /// The receipt could not be encoded.
    Encode(String),
}

impl core::fmt::Display for ReviewError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Input(message) => write!(formatter, "candidate review input: {message}"),
            Self::Encode(message) => write!(formatter, "candidate review encode: {message}"),
        }
    }
}

impl std::error::Error for ReviewError {}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(DIGITS[(byte >> 4) as usize] as char);
        text.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    text
}

fn digest(bytes: &[u8]) -> String {
    format!("blake3:{}", hex(blake3::hash(bytes).as_bytes()))
}

/// Parse one candidate-validation receipt into a review row.
pub fn candidate_row(role: &str, bytes: &[u8]) -> Result<CandidateRow, ReviewError> {
    let receipt: CandidateValidationReceipt = serde_json::from_slice(bytes)
        .map_err(|error| ReviewError::Input(format!("candidate receipt JSON: {error}")))?;
    receipt
        .verify()
        .map_err(|error| ReviewError::Input(format!("candidate receipt invalid: {error:?}")))?;
    let mut candidate_digests: Vec<String> = receipt
        .body
        .candidate
        .iter()
        .map(|artifact| artifact.digest.clone())
        .collect();
    candidate_digests.sort();
    Ok(CandidateRow {
        role: role.to_string(),
        receipt_digest: digest(bytes),
        candidate_digests,
        extra_cmdline: receipt.body.profile.extra_cmdline.clone(),
        completed: receipt.body.outcome.completed,
        bugs_found: receipt.body.outcome.bugs_found,
        checkpoint_digest: receipt.body.outcome.checkpoint_digest.clone(),
    })
}

/// Parse a replay verdict document into review facts.
pub fn replay_facts(bytes: &[u8]) -> Result<ReplayFacts, ReviewError> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| ReviewError::Input(format!("replay verdict JSON: {error}")))?;
    let replay_class = value
        .get("replay_class")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| ReviewError::Input("replay verdict lacks replay_class".to_string()))?;
    let reproduced = value
        .get("reproduced")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| ReviewError::Input("replay verdict lacks reproduced".to_string()))?;
    let snapshot_status = value
        .get("snapshot")
        .and_then(|snapshot| snapshot.get("status"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| ReviewError::Input("replay verdict lacks snapshot.status".to_string()))?;
    Ok(ReplayFacts {
        verdict_digest: digest(bytes),
        replay_class: replay_class.to_string(),
        reproduced,
        snapshot_status: snapshot_status.to_string(),
    })
}

/// Read the branch identity from a probe document.
pub fn branch_identity(bytes: &[u8]) -> Result<String, ReviewError> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| ReviewError::Input(format!("probe JSON: {error}")))?;
    value
        .get("branch_id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ReviewError::Input("probe lacks branch_id".to_string()))
}

fn verdict(
    positive: &CandidateRow,
    negative: &CandidateRow,
    replay: &ReplayFacts,
) -> ReviewVerdict {
    if negative.completed && negative.bugs_found.unwrap_or_default() > 0 {
        return ReviewVerdict::Fail;
    }
    if positive.completed && positive.bugs_found == Some(0) {
        return ReviewVerdict::Fail;
    }
    if replay.reproduced
        && replay.replay_class == "snapshot_backed_reproduced"
        && replay.snapshot_status == "valid"
        && positive.completed
        && positive.bugs_found.unwrap_or_default() > 0
        && negative.completed
        && negative.bugs_found == Some(0)
    {
        ReviewVerdict::Ready
    } else {
        ReviewVerdict::Incomplete
    }
}

fn body_digest(receipt: &CandidateReviewReceipt) -> Result<String, ReviewError> {
    let mut body = receipt.clone();
    body.review_digest.clear();
    let bytes =
        serde_json::to_vec(&body).map_err(|error| ReviewError::Encode(format!("{error}")))?;
    Ok(digest(&bytes))
}

/// Build one reviewer receipt.
#[allow(clippy::too_many_arguments)]
pub fn build(
    original: OriginalFailure,
    branch_id: String,
    replay: ReplayFacts,
    positive: CandidateRow,
    negative: CandidateRow,
) -> Result<CandidateReviewReceipt, ReviewError> {
    if branch_id.is_empty() {
        return Err(ReviewError::Input("empty branch identity".to_string()));
    }
    let verdict = verdict(&positive, &negative, &replay);
    let mut receipt = CandidateReviewReceipt {
        schema: CANDIDATE_REVIEW_SCHEMA.to_string(),
        original,
        branch_id,
        replay,
        positive,
        negative,
        verdict,
        review_digest: String::new(),
        non_claims: vec![
            "A candidate receipt reports one bounded fresh run, not a fix.".to_string(),
            "Replay facts describe the recorded artifact cohort only.".to_string(),
            "The reviewer verdict is not release eligibility or correctness proof.".to_string(),
            "Candidate artifacts and campaign profiles are not interchangeable across cohorts."
                .to_string(),
        ],
    };
    receipt.review_digest = body_digest(&receipt)?;
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn original() -> OriginalFailure {
        OriginalFailure {
            checkpoint_digest: "b3:a".into(),
            artifact: "bug_0.json".into(),
            bug_id: 0,
            dedup_key: Some(1),
            tick: 10,
        }
    }

    fn row(role: &str, completed: bool, bugs: Option<usize>) -> CandidateRow {
        CandidateRow {
            role: role.into(),
            receipt_digest: format!("blake3:{role}"),
            candidate_digests: vec!["b3:c".into()],
            extra_cmdline: String::new(),
            completed,
            bugs_found: bugs,
            checkpoint_digest: Some("b3:d".into()),
        }
    }

    fn replay(reproduced: bool, class: &str, status: &str) -> ReplayFacts {
        ReplayFacts {
            verdict_digest: "blake3:r".into(),
            replay_class: class.into(),
            reproduced,
            snapshot_status: status.into(),
        }
    }

    #[test]
    fn ready_requires_replay_positive_findings_and_a_clean_negative() {
        let receipt = build(
            original(),
            "b3:b".into(),
            replay(true, "snapshot_backed_reproduced", "valid"),
            row("positive", true, Some(2)),
            row("negative", true, Some(0)),
        )
        .unwrap();
        assert_eq!(receipt.verdict, ReviewVerdict::Ready);
        assert!(receipt.review_digest.starts_with("blake3:"));
    }

    #[test]
    fn contradictory_candidates_fail() {
        for (positive, negative, replay) in [
            (
                row("positive", true, Some(2)),
                row("negative", true, Some(1)),
                replay(true, "snapshot_backed_reproduced", "valid"),
            ),
            (
                row("positive", true, Some(0)),
                row("negative", true, Some(0)),
                replay(true, "snapshot_backed_reproduced", "valid"),
            ),
        ] {
            let receipt = build(original(), "b3:b".into(), replay, positive, negative).unwrap();
            assert_eq!(receipt.verdict, ReviewVerdict::Fail);
        }
    }

    #[test]
    fn missing_replay_or_runs_is_incomplete() {
        for replay in [
            replay(false, "snapshot_backed_reproduced", "valid"),
            replay(true, "bounded_smoke", "valid"),
            replay(true, "snapshot_backed_reproduced", "missing_artifact"),
        ] {
            let receipt = build(
                original(),
                "b3:b".into(),
                replay,
                row("positive", true, Some(1)),
                row("negative", true, Some(0)),
            )
            .unwrap();
            assert_eq!(receipt.verdict, ReviewVerdict::Incomplete);
        }
        let receipt = build(
            original(),
            "b3:b".into(),
            replay(true, "snapshot_backed_reproduced", "valid"),
            row("positive", false, None),
            row("negative", false, None),
        )
        .unwrap();
        assert_eq!(receipt.verdict, ReviewVerdict::Incomplete);
    }

    #[test]
    fn review_digest_changes_with_any_bound_fact() {
        let mut receipt = build(
            original(),
            "b3:b".into(),
            replay(true, "snapshot_backed_reproduced", "valid"),
            row("positive", true, Some(1)),
            row("negative", true, Some(0)),
        )
        .unwrap();
        let first = receipt.review_digest.clone();
        receipt.positive.bugs_found = Some(3);
        assert_ne!(first, body_digest(&receipt).unwrap());
    }
}
