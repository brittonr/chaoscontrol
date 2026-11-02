//! Bounded operator status for admitted protocol cohorts.
use super::marker::MarkerBinding;
use super::oracle::OracleVerdict;
use super::{error, CohortResult, Completeness, ProtocolObservation, ProtocolProfile};
use crate::SmrResult;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const STATUS_SCHEMA: &str = "chaoscontrol.protocol-cohort-status.v1";
pub const MAX_GAP_ENTRIES: usize = 64;

/// Operator-facing summary over one admitted cohort and its optional results.
///
/// Missing, partial, and conflicting facts stay visible; this summary never
/// promotes them to a passing protocol result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CohortStatus {
    pub schema: String,
    pub boundary_ref: String,
    pub cohort_ref: String,
    pub completeness: Completeness,
    pub participants_required: usize,
    pub participants_present: usize,
    pub present_participants: Vec<String>,
    pub missing_participants: Vec<String>,
    pub sequence_gaps: Vec<String>,
    pub gaps_truncated: bool,
    pub novelty_ref: String,
    pub oracle_verdict: Option<OracleVerdict>,
    pub marker_reachable: Option<bool>,
    pub blockers: Vec<String>,
}

/// Summarize one cohort without inventing observations or oracle results.
///
/// # Errors
///
/// Rejects an unadmitted profile and a cohort or oracle result from another
/// campaign.
// r[impl chaoscontrol.protocol_observation.evidence]
pub fn protocol_status(
    profile: &ProtocolProfile,
    cohort: &CohortResult,
    records: &[ProtocolObservation],
    oracle: Option<&super::oracle::OracleResult>,
    marker: Option<&MarkerBinding>,
) -> SmrResult<CohortStatus> {
    let profile_ref = profile.admit()?;
    if cohort.profile_ref != profile_ref {
        return Err(error("conflicting", "cohort profile differs from status"));
    }
    if let Some(result) = oracle {
        if result.cohort_ref != cohort.cohort_ref {
            return Err(error("conflicting", "oracle result cohort differs"));
        }
    }
    let mut present = BTreeSet::new();
    let mut sequences: std::collections::BTreeMap<&str, BTreeSet<u64>> =
        std::collections::BTreeMap::new();
    for record in records {
        if record.boundary_ref != cohort.boundary_ref {
            continue;
        }
        present.insert(record.participant_ref.as_str());
        sequences
            .entry(record.participant_ref.as_str())
            .or_default()
            .insert(record.source_sequence);
    }
    let required: BTreeSet<&str> = profile
        .participants
        .iter()
        .map(|p| p.participant_ref.as_str())
        .collect();
    let missing: Vec<String> = required
        .difference(&present)
        .map(|name| (*name).to_string())
        .collect();
    let mut gaps = Vec::new();
    let mut truncated = false;
    for (participant, seen) in &sequences {
        let highest = seen.iter().next_back().copied().unwrap_or(0);
        for sequence in 1..=highest {
            if !seen.contains(&sequence) {
                if gaps.len() == MAX_GAP_ENTRIES {
                    truncated = true;
                    break;
                }
                gaps.push(format!("{participant}#{sequence}"));
            }
        }
        if truncated {
            break;
        }
    }
    Ok(CohortStatus {
        schema: STATUS_SCHEMA.into(),
        boundary_ref: cohort.boundary_ref.clone(),
        cohort_ref: cohort.cohort_ref.clone(),
        completeness: cohort.completeness,
        participants_required: required.len(),
        participants_present: required.intersection(&present).count(),
        present_participants: present
            .iter()
            .filter(|name| required.contains(*name))
            .map(|name| (*name).to_string())
            .collect(),
        missing_participants: missing,
        sequence_gaps: gaps,
        gaps_truncated: truncated,
        novelty_ref: cohort.novelty_ref.clone(),
        oracle_verdict: oracle.map(|result| result.verdict),
        marker_reachable: marker.map(MarkerBinding::complete_protocol_state),
        blockers: cohort.blockers.clone(),
    })
}
