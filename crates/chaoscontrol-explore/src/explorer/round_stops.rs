//! The legacy post-round stop order as a pure rule, shared by the Explorer run
//! loop and frontier-parity replay so both evaluate stops identically.

use super::ExplorationStop;

/// Runs with fewer rounds than this stop at their first finding.
pub(crate) const SHORT_RUN_ROUND_LIMIT: u64 = 10;

/// Facts the run loop checks after one completed round, in legacy order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PostRoundFacts {
    /// The frontier is empty after the round and after any legacy corpus recycling.
    pub(crate) frontier_exhausted: bool,
    /// Consecutive rounds with no new edges and no new findings, this one included.
    pub(crate) consecutive_stale_rounds: u64,
    /// Stale rounds that stop the run; `0` disables the stop.
    pub(crate) stale_round_limit: u64,
    /// New findings of this round.
    pub(crate) round_findings: usize,
    /// Configured round bound.
    pub(crate) max_rounds: u64,
    /// An operator signal requested shutdown.
    pub(crate) shutdown_requested: bool,
}

/// First stop that fires after a round: frontier exhaustion, coverage plateau,
/// short-run finding, then operator signal.
pub(crate) const fn post_round_stop(facts: PostRoundFacts) -> Option<ExplorationStop> {
    if facts.frontier_exhausted {
        return Some(ExplorationStop::FrontierExhausted);
    }
    if facts.stale_round_limit > 0 && facts.consecutive_stale_rounds >= facts.stale_round_limit {
        return Some(ExplorationStop::CoveragePlateau);
    }
    if facts.round_findings > 0 && facts.max_rounds < SHORT_RUN_ROUND_LIMIT {
        return Some(ExplorationStop::BugFound);
    }
    if facts.shutdown_requested {
        return Some(ExplorationStop::Interrupted);
    }
    None
}

/// Stale-round count after a round with `new_edges` and `new_findings`.
pub(crate) const fn next_stale_rounds(previous: u64, new_edges: usize, new_findings: usize) -> u64 {
    if new_edges == 0 && new_findings == 0 {
        previous.saturating_add(1)
    } else {
        0
    }
}

/// Whether a run that completed `rounds_completed` rounds ran its full bound.
pub(crate) const fn completed_every_round(rounds_completed: u64, max_rounds: u64) -> bool {
    rounds_completed >= max_rounds
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUIET: PostRoundFacts = PostRoundFacts {
        frontier_exhausted: false,
        consecutive_stale_rounds: 0,
        stale_round_limit: 3,
        round_findings: 0,
        max_rounds: SHORT_RUN_ROUND_LIMIT,
        shutdown_requested: false,
    };

    #[test]
    fn stops_fire_in_legacy_precedence() {
        assert_eq!(post_round_stop(QUIET), None);
        let every = PostRoundFacts {
            frontier_exhausted: true,
            consecutive_stale_rounds: 3,
            round_findings: 1,
            max_rounds: SHORT_RUN_ROUND_LIMIT - 1,
            shutdown_requested: true,
            ..QUIET
        };
        assert_eq!(
            post_round_stop(every),
            Some(ExplorationStop::FrontierExhausted)
        );
        let plateau = PostRoundFacts {
            frontier_exhausted: false,
            ..every
        };
        assert_eq!(
            post_round_stop(plateau),
            Some(ExplorationStop::CoveragePlateau)
        );
        let finding = PostRoundFacts {
            consecutive_stale_rounds: 2,
            ..plateau
        };
        assert_eq!(post_round_stop(finding), Some(ExplorationStop::BugFound));
        let signal = PostRoundFacts {
            round_findings: 0,
            ..finding
        };
        assert_eq!(post_round_stop(signal), Some(ExplorationStop::Interrupted));
    }

    #[test]
    fn boundaries_disable_their_stops() {
        // A zero limit never plateaus.
        let disabled = PostRoundFacts {
            consecutive_stale_rounds: u64::MAX,
            stale_round_limit: 0,
            ..QUIET
        };
        assert_eq!(post_round_stop(disabled), None);
        // A finding stops only runs shorter than the short-run limit.
        let long_run = PostRoundFacts {
            round_findings: 1,
            max_rounds: SHORT_RUN_ROUND_LIMIT,
            ..QUIET
        };
        assert_eq!(post_round_stop(long_run), None);
        // The plateau fires exactly at the limit.
        let below = PostRoundFacts {
            consecutive_stale_rounds: 2,
            ..QUIET
        };
        assert_eq!(post_round_stop(below), None);
        assert_eq!(next_stale_rounds(2, 0, 0), 3);
        assert_eq!(next_stale_rounds(2, 1, 0), 0);
        assert_eq!(next_stale_rounds(2, 0, 1), 0);
        assert_eq!(next_stale_rounds(u64::MAX, 0, 0), u64::MAX);
        assert!(completed_every_round(0, 0));
        assert!(!completed_every_round(2, 3));
    }
}
