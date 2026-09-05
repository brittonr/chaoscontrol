//! Protocol cohorts remain separate from process-local assertion guidance.

mod session;
use crate::coverage::{CoverageBitmap, ASSERTION_REGION_END, CODE_REGION_END};
use chaoscontrol_fault::protocol_collection::Collection;
use chaoscontrol_protocol::protocol_observation::*;
pub use session::{snapshot_binding_reference, ReplayRequest, Session, SessionError};

pub fn collect_cohort(
    profile: &AdmittedProfile,
    boundary: &str,
    collections: &[&Collection],
) -> Result<CohortResult, ProtocolObservationError> {
    validate_profile_identity(profile)?;
    if collections.len() > profile.profile.bounds.max_participants as usize {
        return Err(ProtocolObservationError::BoundExceeded("collections"));
    }
    let mut records = Vec::new();
    let mut loss_count = 0_u64;
    for collection in collections {
        collection.validate()?;
        if collection.admitted_profile()? != *profile {
            return Err(ProtocolObservationError::IdentityMismatch(
                "collection-profile",
            ));
        }
        loss_count = loss_count
            .checked_add(collection.rejected())
            .ok_or(ProtocolObservationError::CardinalityOverflow)?;
        let total = records
            .len()
            .checked_add(collection.records().len())
            .ok_or(ProtocolObservationError::CardinalityOverflow)?;
        if total > profile.profile.bounds.max_cohort_backlog as usize {
            return Err(ProtocolObservationError::BoundExceeded("cohort-backlog"));
        }
        records.extend_from_slice(collection.records());
    }
    assemble_with_losses(
        profile,
        boundary,
        &records,
        ProjectionSupport::Available,
        loss_count,
    )
}

/// Add bounded guidance only after admission. Return full identities, including slot collisions.
pub fn enrich_coverage(
    profile: &AdmittedProfile,
    cohort: &CohortResult,
    coverage: &mut CoverageBitmap,
) -> Result<Vec<String>, ProtocolObservationError> {
    validate_cohort(profile, cohort)?;
    if cohort.classification != CohortClassification::Complete {
        return Err(ProtocolObservationError::CohortNotComplete);
    }
    let region_size = ASSERTION_REGION_END - CODE_REGION_END;
    let slots: Result<Vec<_>, _> = cohort
        .novelty_identities
        .iter()
        .map(|identity| novelty_coverage_slot(identity, CODE_REGION_END, region_size))
        .collect();
    for slot in slots? {
        coverage.record_hit(slot);
    }
    Ok(cohort.novelty_identities.clone())
}

pub fn validate_replay(
    profile: &AdmittedProfile,
    expected: &CohortResult,
    actual: &CohortResult,
) -> Result<(), ProtocolObservationError> {
    validate_cohort(profile, expected)?;
    validate_cohort(profile, actual)?;
    if expected.classification != CohortClassification::Complete || *expected != *actual {
        return Err(ProtocolObservationError::IdentityMismatch(
            "protocol-replay",
        ));
    }
    Ok(())
}
