//! Pure explorer seed-repeatability comparison.
//!
//! Compares per-round branch digests from two runs of the same configuration.
//! Runs with watchdog firings, discarded branches, or failed branches are
//! operational, not repeatability mismatches. Old checkpoints without digests
//! are unavailable rather than silently matching.

use crate::explorer::RoundHistory;
use serde::{Deserialize, Serialize};

/// Comparison schema identifier.
pub const SCHEMA: &str = "chaoscontrol.explorer_repeatability.v1";

/// Why a round cannot support a repeatability claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OperationalReason {
    /// At least one run reported host watchdog firings.
    WatchdogFirings,
    /// At least one run discarded branches.
    DiscardedBranches,
    /// At least one run excluded failed branches.
    FailedBranches,
}

/// Why two rounds differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MismatchReason {
    /// The two runs have different branch counts in the round.
    BranchCount,
    /// One branch digest differs.
    BranchDigest,
}

/// Repeatability verdict for two runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "kebab-case", deny_unknown_fields)]
pub enum RepeatabilityVerdict {
    /// Every compared round matches.
    Match,
    /// The runs recorded different round counts.
    RoundCountMismatch {
        /// Rounds in the first run.
        first: usize,
        /// Rounds in the second run.
        second: usize,
    },
    /// A round differs.
    Mismatch {
        /// 1-indexed round number.
        round: u64,
        /// Branch position within the round, when known.
        #[serde(skip_serializing_if = "Option::is_none")]
        branch: Option<usize>,
        /// Difference class.
        reason: MismatchReason,
    },
    /// A round is excluded from repeatability claims.
    Operational {
        /// 1-indexed round number.
        round: u64,
        /// Exclusion cause.
        reason: OperationalReason,
    },
    /// A round has no recorded branch digests.
    Unavailable {
        /// 1-indexed round number.
        round: u64,
    },
}

fn operational_reason(history: &RoundHistory) -> Option<OperationalReason> {
    if history.host_watchdog_timeouts > 0 {
        return Some(OperationalReason::WatchdogFirings);
    }
    if history.host_watchdog_discarded_branches > 0 {
        return Some(OperationalReason::DiscardedBranches);
    }
    if history.failed_branches > 0 {
        return Some(OperationalReason::FailedBranches);
    }
    None
}

// r[impl chaoscontrol.explorer_repeatability.gate]
// r[impl chaoscontrol.explorer_repeatability.operational_exclusion]
/// Compare two runs round by round.
///
/// Operational rounds are excluded before digest comparison. A round whose
/// digests are absent on either side is `Unavailable`; absence never matches.
pub fn compare(first: &[RoundHistory], second: &[RoundHistory]) -> RepeatabilityVerdict {
    if first.len() != second.len() {
        return RepeatabilityVerdict::RoundCountMismatch {
            first: first.len(),
            second: second.len(),
        };
    }
    for (left, right) in first.iter().zip(second) {
        if let Some(reason) = operational_reason(left).or_else(|| operational_reason(right)) {
            return RepeatabilityVerdict::Operational {
                round: left.round,
                reason,
            };
        }
        if left.branch_digests.is_empty() || right.branch_digests.is_empty() {
            return RepeatabilityVerdict::Unavailable { round: left.round };
        }
        if left.branch_digests.len() != right.branch_digests.len() {
            return RepeatabilityVerdict::Mismatch {
                round: left.round,
                branch: Some(left.branch_digests.len().min(right.branch_digests.len())),
                reason: MismatchReason::BranchCount,
            };
        }
        if let Some(branch) = left
            .branch_digests
            .iter()
            .zip(&right.branch_digests)
            .position(|(first_digest, second_digest)| first_digest != second_digest)
        {
            return RepeatabilityVerdict::Mismatch {
                round: left.round,
                branch: Some(branch),
                reason: MismatchReason::BranchDigest,
            };
        }
    }
    RepeatabilityVerdict::Match
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(value: u8) -> String {
        format!("b3:{}", format!("{value:02x}").repeat(32))
    }

    fn history(
        round: u64,
        digests: &[u8],
        watchdog: usize,
        discarded: usize,
        failed: usize,
    ) -> RoundHistory {
        RoundHistory {
            round,
            branches_run: digests.len(),
            failed_branches: failed,
            new_edges: 0,
            cumulative_edges: 0,
            bugs_found: 0,
            cumulative_bugs: 0,
            frontier_size: 0,
            corpus_size: 0,
            restore_ms: 0.0,
            run_ms: 0.0,
            snapshot_ms: 0.0,
            coverage_ms: 0.0,
            wall_clock_seconds: 0.0,
            host_watchdog_timeouts: watchdog,
            host_watchdog_discarded_branches: discarded,
            branch_digests: digests.iter().map(|value| digest(*value)).collect(),
        }
    }

    // r[verify chaoscontrol.explorer_repeatability.gate]
    #[test]
    fn identical_rounds_match() {
        let first = vec![history(1, &[1, 2], 0, 0, 0), history(2, &[3], 0, 0, 0)];
        let second = vec![history(1, &[1, 2], 0, 0, 0), history(2, &[3], 0, 0, 0)];
        assert_eq!(compare(&first, &second), RepeatabilityVerdict::Match);
    }

    #[test]
    fn first_divergent_branch_is_named() {
        let first = vec![history(1, &[1, 2, 3], 0, 0, 0)];
        let second = vec![history(1, &[1, 9, 3], 0, 0, 0)];
        assert_eq!(
            compare(&first, &second),
            RepeatabilityVerdict::Mismatch {
                round: 1,
                branch: Some(1),
                reason: MismatchReason::BranchDigest,
            }
        );
    }

    #[test]
    fn branch_and_round_counts_are_distinct() {
        let first = vec![history(1, &[1], 0, 0, 0)];
        let second = vec![history(1, &[1, 2], 0, 0, 0)];
        assert_eq!(
            compare(&first, &second),
            RepeatabilityVerdict::Mismatch {
                round: 1,
                branch: Some(1),
                reason: MismatchReason::BranchCount,
            }
        );
        assert_eq!(
            compare(&first, &[]),
            RepeatabilityVerdict::RoundCountMismatch {
                first: 1,
                second: 0
            }
        );
    }

    // r[verify chaoscontrol.explorer_repeatability.operational_exclusion]
    #[test]
    fn operational_rounds_are_excluded_before_comparison() {
        let first = vec![history(1, &[1], 1, 0, 0)];
        let second = vec![history(1, &[2], 0, 0, 0)];
        assert_eq!(
            compare(&first, &second),
            RepeatabilityVerdict::Operational {
                round: 1,
                reason: OperationalReason::WatchdogFirings,
            }
        );
        let first = vec![history(1, &[1], 0, 1, 0)];
        let second = vec![history(1, &[1], 0, 0, 0)];
        assert_eq!(
            compare(&first, &second),
            RepeatabilityVerdict::Operational {
                round: 1,
                reason: OperationalReason::DiscardedBranches,
            }
        );
        let first = vec![history(1, &[1], 0, 0, 1)];
        let second = vec![history(1, &[1], 0, 0, 0)];
        assert_eq!(
            compare(&first, &second),
            RepeatabilityVerdict::Operational {
                round: 1,
                reason: OperationalReason::FailedBranches,
            }
        );
    }

    #[test]
    fn missing_digests_are_unavailable_not_matching() {
        let first = vec![history(1, &[], 0, 0, 0)];
        let second = vec![history(1, &[], 0, 0, 0)];
        assert_eq!(
            compare(&first, &second),
            RepeatabilityVerdict::Unavailable { round: 1 }
        );
        let first = vec![history(1, &[1], 0, 0, 0)];
        let second = vec![history(1, &[], 0, 0, 0)];
        assert_eq!(
            compare(&first, &second),
            RepeatabilityVerdict::Unavailable { round: 1 }
        );
    }
}
