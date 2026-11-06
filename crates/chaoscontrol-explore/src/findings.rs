//! Pure finding classification for campaign aggregation.
//!
//! A branch may only turn an observed safety violation into a bug record.
//! `sometimes` and `reachable` records are campaign-level liveness or coverage
//! facts: a branch that did not satisfy them is a candidate, not a bug.
//!
//! This module does not change assertion identity, replay candidates, or
//! coverage enrichment.
// r[impl chaoscontrol.exploration_findings.safety_bugs]

use chaoscontrol_fault::oracle::AssertionRecord;
use chaoscontrol_protocol::identity::{AssertionFingerprint, AssertionKind};
use serde::{Deserialize, Serialize};

/// Classification of one assertion record from one branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchFinding {
    /// An observed safety violation: `always` was false or `unreachable` was
    /// reached.
    SafetyViolation,
    /// A `sometimes` or `reachable` record this branch did not satisfy.
    /// Only campaign-wide aggregation can decide whether it is a gap.
    LivenessCandidate,
    /// The branch observed the expected condition (or none was required).
    Satisfied,
    /// The record was not evaluated in this branch.
    Unexercised,
}

/// Classify one branch-local assertion record.
pub fn classify(record: &AssertionRecord) -> BranchFinding {
    match record.kind {
        AssertionKind::Always => {
            if record.hit_count == 0 {
                BranchFinding::Unexercised
            } else if record.false_count > 0 {
                BranchFinding::SafetyViolation
            } else {
                BranchFinding::Satisfied
            }
        }
        AssertionKind::Unreachable => {
            if record.hit_count > 0 {
                BranchFinding::SafetyViolation
            } else {
                BranchFinding::Unexercised
            }
        }
        AssertionKind::Sometimes => {
            if record.hit_count == 0 {
                BranchFinding::Unexercised
            } else if record.true_count > 0 {
                BranchFinding::Satisfied
            } else {
                BranchFinding::LivenessCandidate
            }
        }
        AssertionKind::Reachable => {
            if record.hit_count > 0 {
                BranchFinding::Satisfied
            } else {
                BranchFinding::LivenessCandidate
            }
        }
    }
}

/// One campaign-level liveness or coverage gap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignLivenessGap {
    /// Assertion fingerprint.
    pub fingerprint: AssertionFingerprint,
    /// Assertion kind that produced the gap.
    pub kind: AssertionKind,
    /// Assertion message.
    pub message: String,
    /// Evaluation count of the retained unsatisfied branch observation.
    pub hit_count: u64,
}

pub(crate) fn validate_liveness_gaps(gaps: &[CampaignLivenessGap]) -> Result<(), String> {
    let mut fingerprints = std::collections::BTreeSet::new();
    for gap in gaps {
        if !fingerprints.insert(gap.fingerprint)
            || !matches!(
                (gap.kind, gap.hit_count),
                (AssertionKind::Sometimes, 1..) | (AssertionKind::Reachable, 0)
            )
            || gap.message.is_empty()
            || gap.message.len() > chaoscontrol_protocol::identity::MAX_ASSERTION_MESSAGE_BYTES
            || gap.message.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err("campaign liveness gap metadata is invalid".to_string());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chaoscontrol_protocol::identity::AssertionKind;
    use std::collections::BTreeSet;

    fn record(kind: AssertionKind, hit: u64, truthy: u64, falsy: u64) -> AssertionRecord {
        AssertionRecord {
            message: "assertion".to_string(),
            kind,
            hit_count: hit,
            true_count: truthy,
            false_count: falsy,
            runs_hit: 1,
            runs_satisfied: 0,
            first_failure_run: None,
            last_failure_details: None,
            guest: "guest".to_string(),
            category: "invariant".to_string(),
            identity: None,
            compatibility_id: None,
            must_hit: None,
            catalog_tokens: BTreeSet::new(),
            vm_instances: BTreeSet::new(),
            process_instances: BTreeSet::new(),
            fallback_scope: None,
        }
    }

    // r[verify chaoscontrol.exploration_findings.safety_bugs]
    #[test]
    fn always_is_a_bug_only_when_observed_false() {
        assert_eq!(
            classify(&record(AssertionKind::Always, 5, 4, 1)),
            BranchFinding::SafetyViolation
        );
        assert_eq!(
            classify(&record(AssertionKind::Always, 5, 5, 0)),
            BranchFinding::Satisfied
        );
        assert_eq!(
            classify(&record(AssertionKind::Always, 0, 0, 0)),
            BranchFinding::Unexercised
        );
    }

    #[test]
    fn unreachable_is_a_bug_only_when_reached() {
        assert_eq!(
            classify(&record(AssertionKind::Unreachable, 1, 1, 0)),
            BranchFinding::SafetyViolation
        );
        assert_eq!(
            classify(&record(AssertionKind::Unreachable, 0, 0, 0)),
            BranchFinding::Unexercised
        );
    }

    // r[verify chaoscontrol.exploration_findings.campaign_liveness]
    #[test]
    fn sometimes_and_reachable_are_liveness_candidates_not_bugs() {
        assert_eq!(
            classify(&record(AssertionKind::Sometimes, 3, 0, 3)),
            BranchFinding::LivenessCandidate
        );
        assert_eq!(
            classify(&record(AssertionKind::Sometimes, 3, 1, 2)),
            BranchFinding::Satisfied
        );
        assert_eq!(
            classify(&record(AssertionKind::Sometimes, 0, 0, 0)),
            BranchFinding::Unexercised
        );
        assert_eq!(
            classify(&record(AssertionKind::Reachable, 0, 0, 0)),
            BranchFinding::LivenessCandidate
        );
        assert_eq!(
            classify(&record(AssertionKind::Reachable, 2, 2, 0)),
            BranchFinding::Satisfied
        );
    }
}
