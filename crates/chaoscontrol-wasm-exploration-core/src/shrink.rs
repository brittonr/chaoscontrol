//! Bounded shrink admission and ledger binding.
//!
//! A shrink step is admitted only when the candidate artifact is smaller and
//! the failure predicate still holds. Every accepted step and the final
//! candidate land in a ledger so minimization stays reconstructable.

use serde::{Deserialize, Serialize};

use crate::classify::OutcomeClass;
use crate::compare::StrategyComparison;
use crate::predicate::{predicate_holds, FailurePredicate};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShrinkCandidate {
    pub candidate_blake3: String,
    pub artifact_size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShrinkStep {
    pub attempt: usize,
    pub candidate: ShrinkCandidate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShrinkRejection {
    AttemptBoundReached,
    NotSmaller,
    PredicateChanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShrinkLedger {
    pub case_id: String,
    pub original_candidate: ShrinkCandidate,
    pub predicate: FailurePredicate,
    pub accepted_steps: Vec<ShrinkStep>,
    pub final_candidate: ShrinkCandidate,
}

/// Admits one shrink step. Ordering is explicit: the attempt bound is
/// enforced first (mapping to a typed bound skip), then the size rule, then
/// the stable failure predicate.
pub fn admit_shrink_step(
    predicate: &FailurePredicate,
    original: &ShrinkCandidate,
    candidate: &ShrinkCandidate,
    candidate_comparison: &StrategyComparison,
    attempts_used: usize,
    maximum_shrink_attempts: usize,
) -> Result<ShrinkStep, ShrinkRejection> {
    if maximum_shrink_attempts == 0 || attempts_used >= maximum_shrink_attempts {
        return Err(ShrinkRejection::AttemptBoundReached);
    }
    if candidate.artifact_size_bytes >= original.artifact_size_bytes {
        return Err(ShrinkRejection::NotSmaller);
    }
    if !predicate_holds(predicate, candidate_comparison) {
        return Err(ShrinkRejection::PredicateChanged);
    }
    Ok(ShrinkStep {
        attempt: attempts_used + 1,
        candidate: candidate.clone(),
    })
}

impl ShrinkRejection {
    /// The evidence class a rejection contributes; exhausting the shrink
    /// attempt bound is a bound skip, other rejections only end the local
    /// minimization attempt.
    pub fn outcome_class(&self) -> OutcomeClass {
        match self {
            ShrinkRejection::AttemptBoundReached => OutcomeClass::BoundSkip,
            ShrinkRejection::NotSmaller | ShrinkRejection::PredicateChanged => {
                OutcomeClass::ShrinkComplete
            }
        }
    }
}

/// Opens a ledger for one shrinking run. The original candidate is the
/// initial final candidate; only accepted steps replace it.
pub fn open_shrink_ledger(
    case_id: &str,
    predicate: FailurePredicate,
    original: ShrinkCandidate,
) -> ShrinkLedger {
    ShrinkLedger {
        case_id: case_id.to_string(),
        final_candidate: original.clone(),
        original_candidate: original,
        predicate,
        accepted_steps: Vec::new(),
    }
}

/// Records an already admitted step in the ledger.
pub fn record_accepted_step(ledger: &mut ShrinkLedger, step: ShrinkStep) {
    ledger.final_candidate = step.candidate.clone();
    ledger.accepted_steps.push(step);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::{compare_strategies, completed_observation, Strategy, StrategyRun};
    use crate::predicate::divergence_predicate;

    fn diverging_comparison() -> StrategyComparison {
        let mut pulley = completed_observation("1");
        pulley.result_values = vec!["2".to_string()];
        compare_strategies(
            "component-case-0001",
            "identity-blake3-value",
            StrategyRun {
                strategy: Strategy::Cranelift,
                observation: completed_observation("1"),
            },
            StrategyRun {
                strategy: Strategy::Pulley,
                observation: pulley,
            },
        )
        .expect("comparison")
    }

    fn candidate(size: u64) -> ShrinkCandidate {
        ShrinkCandidate {
            candidate_blake3: crate::blake3_hex(&size.to_le_bytes()),
            artifact_size_bytes: size,
        }
    }

    #[test]
    fn smaller_reproducing_candidates_are_accepted_in_order() {
        let predicate = divergence_predicate(&diverging_comparison()).expect("predicate");
        let original = candidate(100);
        let smaller = candidate(90);
        let step = admit_shrink_step(
            &predicate,
            &original,
            &smaller,
            &diverging_comparison(),
            0,
            4096,
        )
        .expect("step must be admitted");
        assert_eq!(step.attempt, 1);

        let mut ledger = open_shrink_ledger("component-case-0001", predicate, original);
        record_accepted_step(&mut ledger, step);
        let step = admit_shrink_step(
            &ledger.predicate,
            &ledger.original_candidate,
            &candidate(80),
            &diverging_comparison(),
            ledger.accepted_steps.len(),
            4096,
        )
        .expect("second step must be admitted");
        assert_eq!(step.attempt, 2);
        record_accepted_step(&mut ledger, step);
        assert_eq!(ledger.accepted_steps.len(), 2);
        assert_eq!(ledger.final_candidate.artifact_size_bytes, 80);
        assert_eq!(ledger.original_candidate.artifact_size_bytes, 100);
    }

    #[test]
    fn larger_equal_or_class_drifting_candidates_are_rejected() {
        let predicate = divergence_predicate(&diverging_comparison()).expect("predicate");
        let original = candidate(100);
        let comparison = diverging_comparison();

        let error = admit_shrink_step(&predicate, &original, &candidate(100), &comparison, 0, 4096)
            .expect_err("equal size must be rejected");
        assert_eq!(error, ShrinkRejection::NotSmaller);

        let error = admit_shrink_step(&predicate, &original, &candidate(120), &comparison, 0, 4096)
            .expect_err("larger size must be rejected");
        assert_eq!(error, ShrinkRejection::NotSmaller);

        let mut drifted = diverging_comparison();
        drifted.case_id = String::from("component-case-0002");
        let error = admit_shrink_step(&predicate, &original, &candidate(90), &drifted, 0, 4096)
            .expect_err("class drift must be rejected");
        assert_eq!(error, ShrinkRejection::PredicateChanged);
        assert_eq!(error.outcome_class(), OutcomeClass::ShrinkComplete);
    }

    #[test]
    fn exhausting_the_attempt_bound_is_a_typed_bound_skip() {
        let predicate = divergence_predicate(&diverging_comparison()).expect("predicate");
        let original = candidate(100);
        let error = admit_shrink_step(
            &predicate,
            &original,
            &candidate(90),
            &diverging_comparison(),
            4096,
            4096,
        )
        .expect_err("exhausted bound must be rejected");
        assert_eq!(error, ShrinkRejection::AttemptBoundReached);
        assert_eq!(error.outcome_class(), OutcomeClass::BoundSkip);

        let error = admit_shrink_step(
            &predicate,
            &original,
            &candidate(90),
            &diverging_comparison(),
            0,
            0,
        )
        .expect_err("zero attempt bound must be rejected");
        assert_eq!(error, ShrinkRejection::AttemptBoundReached);
    }
}
