//! Normalized strategy comparison over the seven observation fields.
//!
//! Two admitted strategy runs of the same case compare field by field in the
//! profile's canonical observation order. The first differing field names the
//! divergence; nothing else (timing, diagnostics, backend logs) is a
//! comparison surface.

use serde::{Deserialize, Serialize};

use crate::classify::OutcomeClass;

/// One pinned Wasmtime execution strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Strategy {
    Cranelift,
    Pulley,
}

impl Strategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Strategy::Cranelift => "cranelift",
            Strategy::Pulley => "pulley",
        }
    }
}

/// The normalized observation of one completed strategy run. Field names and
/// order mirror `REQUIRED_OBSERVATION_FIELDS` in the v1 profile contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NormalizedObservation {
    pub outcome: OutcomeClass,
    pub trap_class: Option<String>,
    pub resource_class: Option<String>,
    pub result_values: Vec<String>,
    pub preserves_output: Option<String>,
    pub hostcall_transcript: Vec<String>,
    pub state_identity_blake3: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrategyRun {
    pub strategy: Strategy,
    pub observation: NormalizedObservation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDifference {
    pub field: String,
    pub left: String,
    pub right: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrategyComparison {
    pub case_id: String,
    pub case_identity_blake3: String,
    pub left: StrategyRun,
    pub right: StrategyRun,
    pub outcome: OutcomeClass,
    pub first_difference: Option<FieldDifference>,
}

fn observation_fields(observation: &NormalizedObservation) -> Vec<(&'static str, String)> {
    vec![
        (
            "outcome",
            serde_json::to_string(&observation.outcome).unwrap_or_default(),
        ),
        (
            "trap-class",
            serde_json::to_string(&observation.trap_class).unwrap_or_default(),
        ),
        (
            "resource-class",
            serde_json::to_string(&observation.resource_class).unwrap_or_default(),
        ),
        (
            "result-values",
            serde_json::to_string(&observation.result_values).unwrap_or_default(),
        ),
        (
            "preserves-output",
            serde_json::to_string(&observation.preserves_output).unwrap_or_default(),
        ),
        (
            "hostcall-transcript",
            serde_json::to_string(&observation.hostcall_transcript).unwrap_or_default(),
        ),
        (
            "state-identity-blake3",
            serde_json::to_string(&observation.state_identity_blake3).unwrap_or_default(),
        ),
    ]
}

fn first_difference(
    left: &NormalizedObservation,
    right: &NormalizedObservation,
) -> Option<FieldDifference> {
    let left_fields = observation_fields(left);
    let right_fields = observation_fields(right);
    for ((field, left_value), (_, right_value)) in left_fields.into_iter().zip(right_fields) {
        if left_value != right_value {
            return Some(FieldDifference {
                field: field.to_string(),
                left: left_value,
                right: right_value,
            });
        }
    }
    None
}

/// Compares two strategy runs of one case. Runs must use distinct strategies
/// and observations must be run-terminal; agreeing runs classify as
/// `strategy-match` and any field difference classifies as
/// `strategy-divergence` with the first differing field retained.
pub fn compare_strategies(
    case_id: &str,
    case_identity_blake3: &str,
    left: StrategyRun,
    right: StrategyRun,
) -> Result<StrategyComparison, String> {
    if left.strategy == right.strategy {
        return Err(format!(
            "strategy comparison needs two distinct strategies, got {:?} twice",
            left.strategy.as_str()
        ));
    }
    for run in [&left, &right] {
        if !run.observation.outcome.is_run_terminal() {
            return Err(format!(
                "observation outcome {:?} is not a run-terminal class",
                run.observation.outcome.as_str()
            ));
        }
    }
    let difference = first_difference(&left.observation, &right.observation);
    let outcome = if difference.is_none() {
        OutcomeClass::StrategyMatch
    } else {
        OutcomeClass::StrategyDivergence
    };
    Ok(StrategyComparison {
        case_id: case_id.to_string(),
        case_identity_blake3: case_identity_blake3.to_string(),
        left,
        right,
        outcome,
        first_difference: difference,
    })
}

#[cfg(test)]
pub(crate) fn completed_observation(result: &str) -> NormalizedObservation {
    NormalizedObservation {
        outcome: OutcomeClass::DeterministicTrap,
        trap_class: Some("wasm trap: unreachable".to_string()),
        resource_class: Some("fuel-retired".to_string()),
        result_values: vec![result.to_string()],
        preserves_output: Some("<true>".to_string()),
        hostcall_transcript: vec!["sqrt(4) -> 2".to_string()],
        state_identity_blake3: Some(crate::blake3_hex(b"final-state")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(strategy: Strategy, observation: NormalizedObservation) -> StrategyRun {
        StrategyRun {
            strategy,
            observation,
        }
    }

    #[test]
    fn agreeing_runs_classify_as_strategy_match() {
        let comparison = compare_strategies(
            "component-case-0001",
            &crate::blake3_hex(b"case-identity"),
            run(Strategy::Cranelift, completed_observation("1")),
            run(Strategy::Pulley, completed_observation("1")),
        )
        .expect("comparison");
        assert_eq!(comparison.outcome, OutcomeClass::StrategyMatch);
        assert!(comparison.first_difference.is_none());
    }

    #[test]
    fn diverging_runs_keep_the_first_differing_field() {
        let mut pulley = completed_observation("1");
        pulley.result_values = vec!["2".to_string()];
        pulley.trap_class = None;
        let comparison = compare_strategies(
            "component-case-0001",
            &crate::blake3_hex(b"case-identity"),
            run(Strategy::Cranelift, completed_observation("1")),
            run(Strategy::Pulley, pulley),
        )
        .expect("comparison");
        assert_eq!(comparison.outcome, OutcomeClass::StrategyDivergence);
        let difference = comparison.first_difference.expect("difference");
        assert_eq!(difference.field, "trap-class");
        assert_ne!(difference.left, difference.right);
    }

    #[test]
    fn comparison_facts_determine_the_verdict_identically() {
        let left = run(Strategy::Cranelift, completed_observation("1"));
        let right = run(Strategy::Pulley, completed_observation("1"));
        let first = compare_strategies("case", "identity-blake3", left.clone(), right.clone())
            .expect("comparison");
        let second =
            compare_strategies("case", "identity-blake3", left, right).expect("comparison");
        assert_eq!(first, second);
    }

    #[test]
    fn repeated_strategies_and_verdict_observations_are_rejected() {
        let error = compare_strategies(
            "case",
            "identity-blake3",
            run(Strategy::Cranelift, completed_observation("1")),
            run(Strategy::Cranelift, completed_observation("1")),
        )
        .expect_err("same strategy must be rejected");
        assert!(error.contains("distinct strategies"));

        let mut non_terminal = completed_observation("1");
        non_terminal.outcome = OutcomeClass::StrategyMatch;
        let error = compare_strategies(
            "case",
            "identity-blake3",
            run(Strategy::Cranelift, non_terminal),
            run(Strategy::Pulley, completed_observation("1")),
        )
        .expect_err("verdict outcome must be rejected");
        assert!(error.contains("run-terminal"));
    }
}
