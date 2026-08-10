// r[impl chaoscontrol.property_coverage.profile]
// r[impl chaoscontrol.property_coverage.ci]
// r[impl chaoscontrol.property_coverage.boundary]
mod evidence;
mod fault_assertion;
pub mod framework;
mod scheduler_snapshot;
mod virtio;

use serde::Serialize;

use framework::{Counterexample, Lane, PropertyProfile, SuiteReport};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AnyCounterexample {
    pub suite: String,
    pub counterexample: serde_json::Value,
}

impl AnyCounterexample {
    fn from_typed<C: Serialize>(counterexample: Box<Counterexample<C>>) -> Self {
        Self {
            suite: counterexample.suite.clone(),
            counterexample: serde_json::to_value(counterexample)
                .expect("a typed counterexample must serialize"),
        }
    }

    fn scheduler_snapshot(
        counterexample: Box<Counterexample<scheduler_snapshot::Command>>,
    ) -> Self {
        Self::from_typed(counterexample)
    }

    fn fault_assertion(counterexample: Box<Counterexample<fault_assertion::Command>>) -> Self {
        Self::from_typed(counterexample)
    }

    fn virtio(counterexample: Box<Counterexample<virtio::Command>>) -> Self {
        Self::from_typed(counterexample)
    }

    fn evidence(counterexample: Box<Counterexample<evidence::Command>>) -> Self {
        Self::from_typed(counterexample)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaneFailure {
    pub profile: PropertyProfile,
    pub counterexample: AnyCounterexample,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaneReport {
    pub schema_version: u32,
    pub profile: PropertyProfile,
    pub suites: Vec<SuiteReport>,
    pub total_sequences: usize,
    pub total_steps: usize,
    pub rejected_commands: usize,
    pub counterexamples: Vec<AnyCounterexample>,
    pub bounded_claim: String,
    pub non_claims: Vec<String>,
}

pub fn run_lane(lane: Lane) -> Result<LaneReport, Box<LaneFailure>> {
    let selected = framework::profile(lane);
    let run = || -> Result<Vec<SuiteReport>, AnyCounterexample> {
        let mut suites = Vec::with_capacity(selected.target_models.len());
        for target in &selected.target_models {
            let report = match target.as_str() {
                "scheduler-snapshot" => scheduler_snapshot::run(&selected)?,
                "fault-assertion" => fault_assertion::run(&selected)?,
                "virtio" => virtio::run(&selected)?,
                "evidence" => evidence::run(&selected)?,
                _ => {
                    return Err(AnyCounterexample {
                        suite: target.clone(),
                        counterexample: serde_json::Value::String(
                            "property profile names an unknown target model".to_string(),
                        ),
                    });
                }
            };
            suites.push(report);
        }
        Ok(suites)
    };
    let suites = run().map_err(|counterexample| {
        Box::new(LaneFailure {
            profile: selected.clone(),
            counterexample,
        })
    })?;
    let total_sequences = suites.iter().map(|suite| suite.sequences).sum();
    let total_steps = suites.iter().map(|suite| suite.steps).sum();
    let rejected_commands = suites.iter().map(|suite| suite.rejected_commands).sum();
    Ok(LaneReport {
        schema_version: selected.schema_version,
        bounded_claim: selected.claim.clone(),
        non_claims: selected.non_claims.clone(),
        profile: selected,
        suites,
        total_sequences,
        total_steps,
        rejected_commands,
        counterexamples: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_fast_property_lane() {
        let report = run_lane(Lane::Fast).expect("the bounded fast property lane must pass");
        assert!(!report.suites.is_empty());
        assert!(report.total_sequences > 0);
        assert!(report.total_steps >= report.total_sequences);
        assert!(report.counterexamples.is_empty());
    }

    #[test]
    #[ignore = "the deep lane has a separate scheduled CI budget"]
    fn bounded_deep_property_lane() {
        let report = run_lane(Lane::Deep).expect("the bounded deep property lane must pass");
        assert!(!report.suites.is_empty());
        assert!(report.total_sequences > 0);
        assert!(report.total_steps >= report.total_sequences);
        assert!(report.counterexamples.is_empty());
    }
}
