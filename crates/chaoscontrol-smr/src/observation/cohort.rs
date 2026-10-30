//! Pure assembly. Arrival order does not order different producers.
use super::{error, ProtocolObservation, ProtocolProfile};
use crate::{canonical_value_ref, validate_digest_ref, SmrResult};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Drain {
    pub participant_ref: String,
    pub producer_ref: String,
    pub generation: u64,
    pub last_sequence: u64,
    pub dropped: u64,
    pub final_drain: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Completeness {
    Complete,
    Incomplete,
    Conflicting,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CohortResult {
    pub profile_ref: String,
    pub boundary_ref: String,
    pub completeness: Completeness,
    pub record_refs: Vec<String>,
    pub cohort_ref: String,
    pub novelty_ref: String,
    pub blockers: Vec<String>,
}

// r[impl chaoscontrol.protocol_observation.cohort]
pub fn assemble_cohort(
    profile: &ProtocolProfile,
    boundary_ref: &str,
    records: &[ProtocolObservation],
    drains: &[Drain],
) -> SmrResult<CohortResult> {
    let profile_ref = profile.admit()?;
    validate_digest_ref("logical boundary", boundary_ref)?;
    let mut result = CohortResult {
        profile_ref,
        boundary_ref: boundary_ref.into(),
        completeness: Completeness::Complete,
        record_refs: Vec::new(),
        cohort_ref: String::new(),
        novelty_ref: String::new(),
        blockers: Vec::new(),
    };
    if records.len() as u64 > profile.bounds.records || drains.len() > profile.participants.len() {
        result.block(Completeness::Incomplete, "record or drain bound exceeded");
        return finish(result, profile, &[]);
    }
    let mut selected = Vec::new();
    let mut total_bytes = 0u64;
    let mut boundaries = BTreeSet::new();
    for record in records {
        let identity = match record.admit(profile, &result.profile_ref) {
            Ok(identity) => identity,
            Err(e) => {
                let state = if e.class == "conflicting" {
                    Completeness::Conflicting
                } else {
                    Completeness::Unsupported
                };
                result.block(state, &e.to_string());
                continue;
            }
        };
        total_bytes = total_bytes.saturating_add(
            serde_json::to_vec(record)
                .map_err(|e| error("malformed", e.to_string()))?
                .len() as u64,
        );
        boundaries.insert(&record.boundary_ref);
        if record.boundary_ref == boundary_ref {
            selected.push((
                record.participant_ref.clone(),
                record.projection_ref.clone(),
            ));
            result.record_refs.push(identity);
        }
    }
    if total_bytes > profile.bounds.total_bytes
        || boundaries.len() as u64 > profile.bounds.boundaries
        || boundaries.len() as u64 > profile.bounds.cohort_backlog
    {
        result.block(
            Completeness::Incomplete,
            "aggregate observation bound exceeded",
        );
    }
    account_sources(profile, records, drains, &mut result);
    finish(result, profile, &selected)
}
fn account_sources(
    profile: &ProtocolProfile,
    records: &[ProtocolObservation],
    drains: &[Drain],
    result: &mut CohortResult,
) {
    for source in &profile.participants {
        let mut sequences = BTreeMap::new();
        let mut selected = BTreeSet::new();
        for record in records
            .iter()
            .filter(|r| r.participant_ref == source.participant_ref)
        {
            if sequences.insert(record.source_sequence, record).is_some() {
                result.block(Completeness::Conflicting, "duplicate source sequence");
            }
            if record.dropped != 0 {
                result.block(
                    Completeness::Incomplete,
                    "producer reported lost observations",
                );
            }
            if record.boundary_ref == result.boundary_ref {
                selected.insert(&record.projection_ref);
            }
        }
        if selected.is_empty() {
            result.block(
                Completeness::Incomplete,
                "required boundary participant missing",
            );
        } else if selected.len() > 1 {
            result.block(
                Completeness::Conflicting,
                "conflicting boundary projections",
            );
        }
        for (offset, sequence) in sequences.keys().enumerate() {
            if *sequence != offset as u64 + 1 {
                result.block(Completeness::Incomplete, "source sequence gap");
                break;
            }
        }
        let matching: Vec<_> = drains
            .iter()
            .filter(|d| d.participant_ref == source.participant_ref)
            .collect();
        if matching.len() != 1 {
            result.block(Completeness::Incomplete, "missing or duplicate final drain");
            continue;
        }
        let drain = matching[0];
        if drain.producer_ref != source.producer_ref || drain.generation != source.generation {
            result.block(Completeness::Unsupported, "stale or unknown drain source");
        }
        if !drain.final_drain
            || drain.dropped != 0
            || drain.last_sequence != sequences.keys().next_back().copied().unwrap_or(0)
        {
            result.block(
                Completeness::Incomplete,
                "final drain does not close source",
            );
        }
    }
}

impl CohortResult {
    fn block(&mut self, state: Completeness, reason: &str) {
        fn rank(state: Completeness) -> u8 {
            match state {
                Completeness::Complete => 0,
                Completeness::Incomplete => 1,
                Completeness::Unsupported => 2,
                Completeness::Conflicting => 3,
            }
        }
        if rank(state) > rank(self.completeness) {
            self.completeness = state;
        }
        self.blockers.push(reason.into());
    }
}

fn finish(
    mut result: CohortResult,
    profile: &ProtocolProfile,
    selected: &[(String, String)],
) -> SmrResult<CohortResult> {
    result.record_refs.sort();
    let mut projections: Vec<_> = selected.iter().collect();
    projections.sort();
    // r[impl chaoscontrol.protocol_observation.novelty]
    result.novelty_ref = canonical_value_ref(
        b"chaoscontrol.protocol-novelty.v1\0",
        &(
            &profile.novelty_selection_ref,
            &profile.projection_schema_ref,
            projections,
        ),
    )?;
    result.cohort_ref = canonical_value_ref(b"chaoscontrol.protocol-cohort.v1\0", &result)?;
    Ok(result)
}
