//! Candidate-validation receipt for one restore probe.
//!
//! The receipt binds a candidate source revision, the checkpoint's content
//! identity, the probe's bounded result, repeat-drift facts, and expectation
//! result under one BLAKE3 digest. It records observations, not correctness or
//! release eligibility.

use serde::{Deserialize, Serialize};

/// Receipt schema identifier.
pub const PROBE_RECEIPT_SCHEMA: &str = "chaoscontrol.restore-probe-receipt.v1";

/// Facts bound into a probe receipt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProbeReceiptBody {
    /// Receipt schema identifier.
    pub schema: String,
    /// Exact source revision under test.
    pub candidate_revision: String,
    /// BLAKE3 identity of the checkpoint file bytes (lower hex).
    pub checkpoint_digest: String,
    /// Deterministic branch identity of the inspected moment, when bound.
    pub branch_id: Option<String>,
    /// Requested target tick, when a seek target was supplied.
    pub target_tick: Option<u64>,
    /// Whether the seek reached its target tick.
    pub seek_reached: bool,
    // r[impl chaoscontrol.agent_investigation.receipt_seek_intervention_facts]
    /// Classified seek stop reason.
    pub seek_outcome: String,
    /// Number of executed seek rounds.
    pub rounds_run: u64,
    /// Number of register interventions applied to the branch.
    pub intervention_count: u64,
    /// Number of probe runs compared.
    pub repeat_runs: u64,
    /// Whether compared runs drifted; `None` when only one run executed.
    pub drift: Option<bool>,
    /// Whether the expected register matched; `None` when not checked.
    pub expectation_matched: Option<bool>,
    /// Bounded verdict: `pass` or `fail`.
    pub verdict: String,
}

/// Receipt with its canonical identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProbeReceipt {
    /// Bound facts.
    #[serde(flatten)]
    pub body: ProbeReceiptBody,
    /// BLAKE3 identity of the canonical body encoding (lower hex).
    pub receipt_digest: String,
}

/// Rejection for a receipt that cannot bind the supplied facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeReceiptError {
    /// The candidate revision is empty.
    MissingRevision,
    /// The checkpoint digest is not a 64-character lower-hex BLAKE3 value.
    InvalidCheckpointDigest,
    /// The repeat count is outside 1-3.
    InvalidRepeatCount,
    /// The receipt digest does not match the body.
    DigestMismatch,
    /// The seek outcome is not a known classification.
    InvalidSeekOutcome,
}

fn valid_hex_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

impl ProbeReceipt {
    // r[impl chaoscontrol.agent_investigation.restore_candidate_receipt]
    /// Bind the supplied facts and compute the canonical receipt digest.
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        candidate_revision: &str,
        checkpoint_digest: &str,
        branch_id: Option<&str>,
        target_tick: Option<u64>,
        seek_reached: bool,
        seek_outcome: &str,
        rounds_run: u64,
        intervention_count: u64,
        repeat_runs: u64,
        drift: Option<bool>,
        expectation_matched: Option<bool>,
    ) -> Result<Self, ProbeReceiptError> {
        if candidate_revision.trim().is_empty() {
            return Err(ProbeReceiptError::MissingRevision);
        }
        if !valid_hex_digest(checkpoint_digest) {
            return Err(ProbeReceiptError::InvalidCheckpointDigest);
        }
        if !(1..=3).contains(&repeat_runs) {
            return Err(ProbeReceiptError::InvalidRepeatCount);
        }
        if !matches!(
            seek_outcome,
            "reached" | "no_target" | "round_cap" | "all_vms_stopped"
        ) {
            return Err(ProbeReceiptError::InvalidSeekOutcome);
        }
        let verdict = if drift == Some(true)
            || expectation_matched == Some(false)
            || (target_tick.is_some() && !seek_reached)
        {
            "fail"
        } else {
            "pass"
        };
        let body = ProbeReceiptBody {
            schema: PROBE_RECEIPT_SCHEMA.to_string(),
            candidate_revision: candidate_revision.to_string(),
            checkpoint_digest: checkpoint_digest.to_string(),
            branch_id: branch_id.map(str::to_string),
            target_tick,
            seek_reached,
            seek_outcome: seek_outcome.to_string(),
            rounds_run,
            intervention_count,
            repeat_runs,
            drift,
            expectation_matched,
            verdict: verdict.to_string(),
        };
        let receipt_digest = body_digest(&body)?;
        Ok(Self {
            body,
            receipt_digest,
        })
    }

    /// Verify the receipt digest against its body.
    pub fn verify(&self) -> Result<(), ProbeReceiptError> {
        if body_digest(&self.body)? == self.receipt_digest {
            Ok(())
        } else {
            Err(ProbeReceiptError::DigestMismatch)
        }
    }

    /// Canonical JSON encoding, terminated by a newline.
    pub fn to_json(&self) -> Result<String, ProbeReceiptError> {
        serde_json::to_string_pretty(self)
            .map(|mut json| {
                json.push('\n');
                json
            })
            .map_err(|_| ProbeReceiptError::DigestMismatch)
    }
}

fn body_digest(body: &ProbeReceiptBody) -> Result<String, ProbeReceiptError> {
    let encoded = serde_json::to_vec(body).map_err(|_| ProbeReceiptError::DigestMismatch)?;
    Ok(blake3::hash(&encoded).to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000001";

    fn receipt() -> ProbeReceipt {
        ProbeReceipt::build(
            "0123456789abcdef0123456789abcdef01234567",
            DIGEST,
            None,
            Some(1181),
            true,
            "reached",
            3,
            0,
            2,
            Some(false),
            Some(true),
        )
        .unwrap()
    }

    #[test]
    fn receipt_binds_facts_and_verifies() {
        let receipt = receipt();
        assert_eq!(receipt.body.verdict, "pass");
        assert_eq!(receipt.body.rounds_run, 3);
        assert_eq!(receipt.body.repeat_runs, 2);
        receipt.verify().unwrap();
        let json = receipt.to_json().unwrap();
        let decoded: ProbeReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, receipt);
        decoded.verify().unwrap();
    }

    #[test]
    fn drift_or_mismatch_fails_the_verdict() {
        let drifted = ProbeReceipt::build(
            "rev",
            DIGEST,
            None,
            None,
            false,
            "round_cap",
            0,
            0,
            2,
            Some(true),
            None,
        )
        .unwrap();
        assert_eq!(drifted.body.verdict, "fail");
        let mismatched = ProbeReceipt::build(
            "rev",
            DIGEST,
            None,
            Some(5),
            true,
            "reached",
            1,
            0,
            1,
            None,
            Some(false),
        )
        .unwrap();
        assert_eq!(mismatched.body.verdict, "fail");
    }

    #[test]
    fn unreached_target_fails_the_verdict() {
        let receipt = ProbeReceipt::build(
            "rev",
            DIGEST,
            None,
            Some(1181),
            false,
            "round_cap",
            0,
            0,
            1,
            None,
            None,
        )
        .unwrap();
        assert_eq!(receipt.body.verdict, "fail");
        let absent = ProbeReceipt::build(
            "rev",
            DIGEST,
            None,
            None,
            false,
            "no_target",
            0,
            0,
            1,
            None,
            None,
        )
        .unwrap();
        assert_eq!(absent.body.verdict, "pass");
    }

    #[test]
    fn invalid_facts_are_rejected() {
        assert_eq!(
            ProbeReceipt::build("", DIGEST, None, None, true, "reached", 1, 0, 1, None, None),
            Err(ProbeReceiptError::MissingRevision)
        );
        assert_eq!(
            ProbeReceipt::build("rev", "abcd", None, None, true, "reached", 1, 0, 1, None, None,),
            Err(ProbeReceiptError::InvalidCheckpointDigest)
        );
        assert_eq!(
            ProbeReceipt::build("rev", DIGEST, None, None, true, "reached", 1, 0, 4, None, None,),
            Err(ProbeReceiptError::InvalidRepeatCount)
        );
    }

    #[test]
    fn tampered_digest_is_rejected() {
        let mut receipt = receipt();
        receipt.receipt_digest = "0".repeat(64);
        assert_eq!(receipt.verify(), Err(ProbeReceiptError::DigestMismatch));
    }
}
