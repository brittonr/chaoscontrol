//! Stable process exit outcomes for exploration commands.
//!
//! Operators and scripts need to distinguish a clean run, a run that found bugs,
//! an assertion-floor failure, and an operational failure. The mapping is pure and
//! must stay stable across commands; `reproduce` keeps its separate contract.

/// Process outcome for one exploration command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerOutcome {
    /// Exploration completed without candidate bugs.
    Clean,
    /// Exploration completed and found at least one candidate bug.
    BugsFound,
    /// Exploration completed but the assertion-exercise floor failed.
    AssertionFloor,
    /// Exploration failed operationally before producing a trustworthy result.
    Failure,
}

impl ExplorerOutcome {
    /// Stable process exit status.
    pub const fn exit_status(self) -> i32 {
        match self {
            Self::Clean => 0,
            Self::BugsFound => 1,
            Self::Failure => 2,
            Self::AssertionFloor => 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcomes_have_distinct_stable_statuses() {
        assert_eq!(ExplorerOutcome::Clean.exit_status(), 0);
        assert_eq!(ExplorerOutcome::BugsFound.exit_status(), 1);
        assert_eq!(ExplorerOutcome::Failure.exit_status(), 2);
        assert_eq!(ExplorerOutcome::AssertionFloor.exit_status(), 3);
        assert_ne!(
            ExplorerOutcome::Failure.exit_status(),
            ExplorerOutcome::BugsFound.exit_status()
        );
    }
}
