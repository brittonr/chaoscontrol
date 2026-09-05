//! Typed outcome classes for the exploration rail.
//!
//! The thirteen classes are the exact evidence vocabulary pinned by the v1
//! profile contract (`REQUIRED_OUTCOME_CLASSES` in `profile.rs`). Every
//! decision this core emits names one of these classes.

use serde::{Deserialize, Serialize};

/// One evidence outcome class. Serialization is the kebab-case contract
/// spelling (`strategy-match`, `static-rejection`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutcomeClass {
    BoundSkip,
    CompileRejection,
    DeterministicTrap,
    HarnessError,
    LinkRejection,
    ProfileRejection,
    ReplayMismatch,
    ShrinkComplete,
    StaticRejection,
    StrategyDivergence,
    StrategyMatch,
    TransformDivergence,
    TransformMatch,
}

impl OutcomeClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            OutcomeClass::BoundSkip => "bound-skip",
            OutcomeClass::CompileRejection => "compile-rejection",
            OutcomeClass::DeterministicTrap => "deterministic-trap",
            OutcomeClass::HarnessError => "harness-error",
            OutcomeClass::LinkRejection => "link-rejection",
            OutcomeClass::ProfileRejection => "profile-rejection",
            OutcomeClass::ReplayMismatch => "replay-mismatch",
            OutcomeClass::ShrinkComplete => "shrink-complete",
            OutcomeClass::StaticRejection => "static-rejection",
            OutcomeClass::StrategyDivergence => "strategy-divergence",
            OutcomeClass::StrategyMatch => "strategy-match",
            OutcomeClass::TransformDivergence => "transform-divergence",
            OutcomeClass::TransformMatch => "transform-match",
        }
    }

    /// Whether the class describes how one execution run ended. Only these
    /// classes are valid per-run observation outcomes; verdict classes such
    /// as `strategy-match` belong to case-level comparisons instead.
    pub fn is_run_terminal(&self) -> bool {
        matches!(
            self,
            OutcomeClass::BoundSkip
                | OutcomeClass::CompileRejection
                | OutcomeClass::DeterministicTrap
                | OutcomeClass::HarnessError
                | OutcomeClass::LinkRejection
                | OutcomeClass::ProfileRejection
                | OutcomeClass::StaticRejection
        )
    }
}

/// The admission boundary that rejected an artifact before execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AdmissionBoundary {
    Compile,
    Link,
    Profile,
    Static,
}

/// Maps a rejecting boundary to its distinct outcome class. Combining
/// boundaries into one failure class would hide whether a check failed
/// correctly, so each boundary keeps its own class.
pub fn classify_boundary_rejection(boundary: AdmissionBoundary) -> OutcomeClass {
    match boundary {
        AdmissionBoundary::Static => OutcomeClass::StaticRejection,
        AdmissionBoundary::Profile => OutcomeClass::ProfileRejection,
        AdmissionBoundary::Compile => OutcomeClass::CompileRejection,
        AdmissionBoundary::Link => OutcomeClass::LinkRejection,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_CLASSES: [OutcomeClass; 13] = [
        OutcomeClass::BoundSkip,
        OutcomeClass::CompileRejection,
        OutcomeClass::DeterministicTrap,
        OutcomeClass::HarnessError,
        OutcomeClass::LinkRejection,
        OutcomeClass::ProfileRejection,
        OutcomeClass::ReplayMismatch,
        OutcomeClass::ShrinkComplete,
        OutcomeClass::StaticRejection,
        OutcomeClass::StrategyDivergence,
        OutcomeClass::StrategyMatch,
        OutcomeClass::TransformDivergence,
        OutcomeClass::TransformMatch,
    ];

    #[test]
    fn every_class_round_trips_through_the_contract_spelling() {
        let spellings: Vec<String> = ALL_CLASSES
            .iter()
            .map(|class| class.as_str().to_string())
            .collect();
        assert_eq!(
            spellings,
            crate::profile::REQUIRED_OUTCOME_CLASSES
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
        );
        for class in ALL_CLASSES {
            let parsed: OutcomeClass =
                serde_json::from_value(serde_json::json!(class.as_str())).expect("parse");
            assert_eq!(parsed, class);
        }
    }

    #[test]
    fn each_boundary_keeps_its_own_rejection_class() {
        assert_eq!(
            classify_boundary_rejection(AdmissionBoundary::Static),
            OutcomeClass::StaticRejection
        );
        assert_eq!(
            classify_boundary_rejection(AdmissionBoundary::Profile),
            OutcomeClass::ProfileRejection
        );
        assert_eq!(
            classify_boundary_rejection(AdmissionBoundary::Compile),
            OutcomeClass::CompileRejection
        );
        assert_eq!(
            classify_boundary_rejection(AdmissionBoundary::Link),
            OutcomeClass::LinkRejection
        );
    }

    #[test]
    fn only_run_terminal_classes_describe_observations() {
        assert!(OutcomeClass::DeterministicTrap.is_run_terminal());
        assert!(OutcomeClass::BoundSkip.is_run_terminal());
        assert!(OutcomeClass::HarnessError.is_run_terminal());
        assert!(!OutcomeClass::StrategyMatch.is_run_terminal());
        assert!(!OutcomeClass::ShrinkComplete.is_run_terminal());
        assert!(!OutcomeClass::TransformMatch.is_run_terminal());
        assert!(!OutcomeClass::ReplayMismatch.is_run_terminal());
    }
}
