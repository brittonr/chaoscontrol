use super::model::{validate_candidates, CausalityError, CauseCandidate};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttributionObservation {
    pub candidate_id: String,
    pub attempt: u32,
    pub neutralized_reproduced: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttributionRanking {
    pub candidate: CauseCandidate,
    pub attempts: u32,
    pub prevented_failures: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub causal_probability_estimate: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttributionReport {
    pub rankings: Vec<AttributionRanking>,
    pub probable_causes: Vec<String>,
    pub budget: u64,
    pub executions_spent: u64,
    pub partial: bool,
    pub equivalent_without_discriminating_cause: bool,
    pub scope: String,
}

pub fn rank_candidates(
    candidates: &[CauseCandidate],
    observations: &[AttributionObservation],
    budget: u64,
) -> Result<AttributionReport, CausalityError> {
    validate_candidates(candidates)?;
    if budget == 0 {
        return Err(CausalityError::new(
            "causality-budget",
            "attribution budget must be positive",
        ));
    }
    let executions_spent = u64::try_from(observations.len()).map_err(|_| {
        CausalityError::new(
            "causality-budget",
            "attribution observation count exceeds u64",
        )
    })?;
    if executions_spent > budget {
        return Err(CausalityError::new(
            "causality-budget",
            "attribution observations exceed the declared budget",
        ));
    }
    let candidate_map = candidates
        .iter()
        .map(|candidate| (candidate.candidate_id.as_str(), candidate))
        .collect::<BTreeMap<_, _>>();
    let mut grouped: BTreeMap<&str, Vec<&AttributionObservation>> = BTreeMap::new();
    for observation in observations {
        if !candidate_map.contains_key(observation.candidate_id.as_str()) {
            return Err(CausalityError::new(
                "causality-attribution-identity",
                format!(
                    "observation references unknown candidate {:?}",
                    observation.candidate_id
                ),
            ));
        }
        grouped
            .entry(observation.candidate_id.as_str())
            .or_default()
            .push(observation);
    }
    let mut rankings = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let outcomes = grouped
            .get(candidate.candidate_id.as_str())
            .map_or(&[][..], Vec::as_slice);
        for (expected, observation) in outcomes.iter().enumerate() {
            let expected = u32::try_from(expected).map_err(|_| {
                CausalityError::new(
                    "causality-attribution-attempt",
                    "attribution attempt index exceeds u32",
                )
            })?;
            if observation.attempt != expected {
                return Err(CausalityError::new(
                    "causality-attribution-attempt",
                    format!(
                        "candidate {:?} attempt order drifted",
                        candidate.candidate_id
                    ),
                ));
            }
        }
        let attempts = u32::try_from(outcomes.len()).map_err(|_| {
            CausalityError::new(
                "causality-attribution-attempt",
                "attribution attempt count exceeds u32",
            )
        })?;
        let prevented_failures = u32::try_from(
            outcomes
                .iter()
                .filter(|outcome| !outcome.neutralized_reproduced)
                .count(),
        )
        .map_err(|_| {
            CausalityError::new(
                "causality-attribution-attempt",
                "prevented failure count exceeds u32",
            )
        })?;
        let causal_probability_estimate =
            (attempts > 0).then_some(f64::from(prevented_failures) / f64::from(attempts));
        rankings.push(AttributionRanking {
            candidate: candidate.clone(),
            attempts,
            prevented_failures,
            causal_probability_estimate,
        });
    }
    rankings.sort_by(|left, right| {
        right
            .causal_probability_estimate
            .partial_cmp(&left.causal_probability_estimate)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.candidate.class.cmp(&right.candidate.class))
            .then_with(|| {
                left.candidate
                    .candidate_id
                    .cmp(&right.candidate.candidate_id)
            })
    });
    let maximum = rankings
        .iter()
        .filter_map(|ranking| ranking.causal_probability_estimate)
        .fold(0.0_f64, f64::max);
    let probable_causes = if maximum > 0.0 {
        rankings
            .iter()
            .filter(|ranking| ranking.causal_probability_estimate == Some(maximum))
            .map(|ranking| ranking.candidate.candidate_id.clone())
            .collect()
    } else {
        Vec::new()
    };
    let partial = rankings.iter().any(|ranking| ranking.attempts == 0);
    Ok(AttributionReport {
        rankings,
        probable_causes,
        budget,
        executions_spent,
        partial,
        equivalent_without_discriminating_cause: maximum == 0.0
            && !observations.is_empty()
            && !partial,
        scope: "probability estimate from supplied neutralization outcomes; not proof of a unique cause"
            .to_string(),
    })
}
