//! Typed failure predicates and their use in shrink admission.
//!
//! A predicate binds one failure outcome class to the identities that must
//! remain present for a reproduction to count as the same failure. Shrinking
//! accepts a smaller candidate only while the predicate still holds, so a
//! minimization can never silently change a parser rejection into an
//! unrelated trap.

use serde::{Deserialize, Serialize};

use crate::classify::OutcomeClass;
use crate::compare::StrategyComparison;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailurePredicate {
    pub outcome_class: OutcomeClass,
    pub required_identities: Vec<String>,
}

fn comparison_identities(comparison: &StrategyComparison) -> Vec<String> {
    let mut identities = vec![
        format!("case:{}", comparison.case_id),
        format!("case-identity:{}", comparison.case_identity_blake3),
    ];
    if let Some(difference) = &comparison.first_difference {
        identities.push(format!("field:{}", difference.field));
        identities.push(format!("left:{}", difference.left));
        identities.push(format!("right:{}", difference.right));
    }
    identities
}

/// Builds the predicate for a reproducible failure. Only divergent
/// comparisons carry a shrinkable failure; a strategy match has nothing to
/// minimize.
pub fn divergence_predicate(comparison: &StrategyComparison) -> Result<FailurePredicate, String> {
    if comparison.outcome != OutcomeClass::StrategyDivergence {
        return Err(format!(
            "divergence predicates require a strategy-divergence outcome, found {:?}",
            comparison.outcome.as_str()
        ));
    }
    if comparison.first_difference.is_none() {
        return Err("strategy-divergence comparison is missing its first difference".to_string());
    }
    let mut required_identities = comparison_identities(comparison);
    required_identities.sort();
    required_identities.dedup();
    Ok(FailurePredicate {
        outcome_class: comparison.outcome,
        required_identities,
    })
}

/// Whether the candidate comparison still exhibits exactly the declared
/// failure: the same outcome class and every required identity still present.
pub fn predicate_holds(predicate: &FailurePredicate, candidate: &StrategyComparison) -> bool {
    if candidate.outcome != predicate.outcome_class {
        return false;
    }
    let candidate_identities = comparison_identities(candidate);
    predicate
        .required_identities
        .iter()
        .all(|required| candidate_identities.contains(required))
}

/// Whether two predicates declare the same failure.
pub fn same_predicate(left: &FailurePredicate, right: &FailurePredicate) -> bool {
    left == right
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::OutcomeClass;
    use crate::compare::{
        compare_strategies, completed_observation, FieldDifference, Strategy, StrategyRun,
    };

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

    #[test]
    fn divergence_predicates_bind_class_and_difference_identities() {
        let comparison = diverging_comparison();
        let predicate = divergence_predicate(&comparison).expect("predicate");
        assert_eq!(predicate.outcome_class, OutcomeClass::StrategyDivergence);
        assert!(predicate
            .required_identities
            .contains(&String::from("case:component-case-0001")));
        assert!(predicate
            .required_identities
            .iter()
            .any(|identity| identity.starts_with("field:result-values")));
        assert!(same_predicate(
            &predicate,
            &divergence_predicate(&diverging_comparison()).expect("predicate")
        ));
    }

    #[test]
    fn match_comparisons_cannot_produce_predicates() {
        let comparison = compare_strategies(
            "case",
            "identity-blake3-value",
            StrategyRun {
                strategy: Strategy::Cranelift,
                observation: completed_observation("1"),
            },
            StrategyRun {
                strategy: Strategy::Pulley,
                observation: completed_observation("1"),
            },
        )
        .expect("comparison");
        let error = divergence_predicate(&comparison).expect_err("match must be rejected");
        assert!(error.contains("strategy-divergence"));
    }

    #[test]
    fn predicate_holds_only_while_the_failure_is_unchanged() {
        let predicate = divergence_predicate(&diverging_comparison()).expect("predicate");
        assert!(predicate_holds(&predicate, &diverging_comparison()));

        let mut class_drift = diverging_comparison();
        class_drift.outcome = OutcomeClass::StrategyMatch;
        assert!(!predicate_holds(&predicate, &class_drift));

        let mut different_difference = diverging_comparison();
        different_difference.case_id = String::from("component-case-0002");
        assert!(!predicate_holds(&predicate, &different_difference));
    }

    #[test]
    fn observation_field_changes_break_the_predicate_identity() {
        let predicate = divergence_predicate(&diverging_comparison()).expect("predicate");
        let mut changed_values = diverging_comparison();
        changed_values.first_difference = Some(FieldDifference {
            field: String::from("result-values"),
            left: String::from("\"9\""),
            right: String::from("\"7\""),
        });
        assert!(!predicate_holds(&predicate, &changed_values));
    }
}
