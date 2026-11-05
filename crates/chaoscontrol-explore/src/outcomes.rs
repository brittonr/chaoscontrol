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
    // r[impl chaoscontrol.explorer_outcomes.exit_status]
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

/// Format an error and every available source cause as an indented chain.
// r[impl chaoscontrol.explorer_outcomes.error_chain]
pub fn format_error_chain(error: &dyn std::error::Error) -> String {
    let mut output = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        output.push_str("\n  caused by: ");
        output.push_str(&cause.to_string());
        source = cause.source();
    }
    output
}

#[cfg(test)]
mod tests {
    use std::fmt;

    #[derive(Debug)]
    struct Outer(std::io::Error);

    impl fmt::Display for Outer {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "outer")
        }
    }

    impl std::error::Error for Outer {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }
    use super::*;

    #[test]
    fn error_chain_names_every_cause() {
        let error = Outer(std::io::Error::other("inner"));
        let chain = format_error_chain(&error);
        assert!(chain.starts_with("outer"));
        assert!(chain.contains("caused by: inner"));
    }

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
