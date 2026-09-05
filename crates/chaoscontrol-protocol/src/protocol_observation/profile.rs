//! Profile admission and implementation resource ceilings.

use super::*;
use std::collections::BTreeSet;

pub const MAX_PROFILE_BYTES: usize = 262_144;
pub const MAX_COHORT_RECORDS: u32 = 4_096;
pub const MAX_COHORT_BOUNDARIES: u32 = 256;
pub const MAX_ORACLE_WORK_ITEMS: u32 = 4_096;
pub const MAX_TOTAL_PROJECTION_BYTES: u64 = 8_388_608;
pub const MAX_DIAGNOSTIC_REFS: u32 = 32;
pub const MAX_NON_CLAIM_BYTES: usize = 128;
pub const REQUIRED_NON_CLAIMS: [&str; 6] = [
    "does not establish production readiness",
    "does not establish protocol semantics",
    "does not establish release eligibility",
    "does not establish universal correctness",
    "does not infer a cross-producer total order",
    "does not synchronize wall clocks",
];

pub fn decode_profile(bytes: &[u8]) -> Result<AdmittedProfile, ProtocolObservationError> {
    if bytes.len() > MAX_PROFILE_BYTES {
        return Err(ProtocolObservationError::BoundExceeded("profile-bytes"));
    }
    let profile =
        serde_json::from_slice(bytes).map_err(|_| ProtocolObservationError::InvalidSchema)?;
    admit_profile(profile)
}

pub fn admit_profile(
    profile: ProtocolObservationProfile,
) -> Result<AdmittedProfile, ProtocolObservationError> {
    validate(&profile)?;
    let (profile_ref, bounds_ref) = identities(&profile)?;
    Ok(AdmittedProfile {
        profile_ref,
        bounds_ref,
        profile,
    })
}

pub fn validate_profile_identity(
    profile: &AdmittedProfile,
) -> Result<(), ProtocolObservationError> {
    validate(&profile.profile)?;
    let expected = identities(&profile.profile)?;
    if (profile.profile_ref.as_str(), profile.bounds_ref.as_str())
        != (expected.0.as_str(), expected.1.as_str())
    {
        return Err(ProtocolObservationError::IdentityMismatch("profile"));
    }
    Ok(())
}

fn identities(
    profile: &ProtocolObservationProfile,
) -> Result<(String, String), ProtocolObservationError> {
    let bytes = serde_json::to_vec(profile).map_err(|_| ProtocolObservationError::InvalidSchema)?;
    let bounds =
        serde_json::to_vec(&profile.bounds).map_err(|_| ProtocolObservationError::InvalidSchema)?;
    Ok((
        digest_reference(
            "protocol-observation-profile",
            PROFILE_IDENTITY_DOMAIN,
            &bytes,
        ),
        digest_reference(
            "protocol-observation-bounds",
            BOUNDS_IDENTITY_DOMAIN,
            &bounds,
        ),
    ))
}

fn validate(profile: &ProtocolObservationProfile) -> Result<(), ProtocolObservationError> {
    if profile.schema != PROFILE_SCHEMA {
        return Err(ProtocolObservationError::InvalidSchema);
    }
    for (value, prefix) in [
        (&profile.protocol_ref, "protocol"),
        (&profile.execution_ref, "execution"),
        (&profile.projection_schema_ref, "projection-schema"),
        (
            &profile.logical_boundary_schema_ref,
            "logical-boundary-schema",
        ),
        (&profile.cohort_ref, "cohort"),
        (&profile.oracle.adapter_ref, "oracle-adapter"),
    ] {
        validate_exact_reference(value, prefix)?;
    }
    if profile.oracle.authority != OracleAuthority::ConsumerIndependent
        || !profile.oracle.requires_complete_cohort
    {
        return Err(ProtocolObservationError::RuntimeSelfOracle);
    }
    bounded_count(profile.producers.len(), MAX_PROFILE_PRODUCERS, "producers")?;
    bounded_count(
        profile.required_participants.len(),
        MAX_PROFILE_PARTICIPANTS,
        "participants",
    )?;
    bounded_count(
        profile.novelty_selectors.len(),
        MAX_NOVELTY_SELECTORS,
        "novelty-selectors",
    )?;
    bounded_count(profile.non_claims.len(), MAX_NON_CLAIMS, "non-claims")?;
    validate_members(profile)?;
    validate_order(profile)?;
    validate_bounds(profile)?;
    Ok(())
}

fn validate_members(profile: &ProtocolObservationProfile) -> Result<(), ProtocolObservationError> {
    let mut producers = BTreeSet::new();
    let mut participants = BTreeSet::new();
    let mut processes = BTreeSet::new();
    for producer in &profile.producers {
        validate_exact_reference(&producer.producer_ref, "producer")?;
        validate_exact_reference(&producer.participant_ref, "participant")?;
        validate_exact_reference(&producer.process_ref, "process")?;
        if !producers.insert(&producer.producer_ref) {
            return Err(ProtocolObservationError::DuplicateProducer);
        }
        if !participants.insert(&producer.participant_ref)
            || !processes.insert((producer.vm_id, &producer.process_ref))
        {
            return Err(ProtocolObservationError::DuplicateParticipant);
        }
    }
    let required = profile
        .required_participants
        .iter()
        .collect::<BTreeSet<_>>();
    if required.len() != profile.required_participants.len() {
        return Err(ProtocolObservationError::DuplicateParticipant);
    }
    if required != participants {
        return Err(ProtocolObservationError::UnknownParticipant);
    }
    for required in REQUIRED_NON_CLAIMS {
        if !profile.non_claims.iter().any(|claim| claim == required) {
            return Err(ProtocolObservationError::ClaimOverreach);
        }
    }
    if profile
        .non_claims
        .iter()
        .any(|claim| claim.len() > MAX_NON_CLAIM_BYTES || claim.chars().any(char::is_control))
    {
        return Err(ProtocolObservationError::BoundExceeded("non-claim-bytes"));
    }
    Ok(())
}

fn validate_order(profile: &ProtocolObservationProfile) -> Result<(), ProtocolObservationError> {
    if !profile
        .producers
        .iter()
        .map(|producer| &producer.producer_ref)
        .is_sorted()
    {
        return Err(ProtocolObservationError::NonCanonicalOrder("producers"));
    }
    if !profile.required_participants.is_sorted() {
        return Err(ProtocolObservationError::NonCanonicalOrder("participants"));
    }
    strict_order(&profile.novelty_selectors, "novelty-selectors")?;
    strict_order(&profile.non_claims, "non-claims")
}

fn strict_order<T: Ord>(items: &[T], field: &'static str) -> Result<(), ProtocolObservationError> {
    if !items
        .iter()
        .zip(items.iter().skip(1))
        .all(|(left, right)| left < right)
    {
        return Err(ProtocolObservationError::NonCanonicalOrder(field));
    }
    Ok(())
}

fn bounded_count(
    count: usize,
    maximum: usize,
    field: &'static str,
) -> Result<(), ProtocolObservationError> {
    if count == 0 || count > maximum {
        return Err(ProtocolObservationError::BoundExceeded(field));
    }
    Ok(())
}

fn validate_bounds(profile: &ProtocolObservationProfile) -> Result<(), ProtocolObservationError> {
    let bounds = &profile.bounds;
    for (value, maximum, field) in [
        (
            u64::from(bounds.max_records_per_producer),
            u64::from(MAX_COHORT_RECORDS),
            "records-per-producer",
        ),
        (
            u64::from(bounds.max_projection_bytes),
            MAX_INLINE_PROJECTION_BYTES as u64,
            "projection-bytes",
        ),
        (
            bounds.max_total_projection_bytes,
            MAX_TOTAL_PROJECTION_BYTES,
            "total-projection-bytes",
        ),
        (
            u64::from(bounds.max_participants),
            MAX_PROFILE_PARTICIPANTS as u64,
            "participants",
        ),
        (
            u64::from(bounds.max_logical_boundaries),
            u64::from(MAX_COHORT_BOUNDARIES),
            "logical-boundaries",
        ),
        (
            u64::from(bounds.max_cohort_backlog),
            u64::from(MAX_COHORT_RECORDS),
            "cohort-backlog",
        ),
        (
            u64::from(bounds.max_oracle_work_items),
            u64::from(MAX_ORACLE_WORK_ITEMS),
            "oracle-work",
        ),
        (
            u64::from(bounds.max_diagnostic_refs),
            u64::from(MAX_DIAGNOSTIC_REFS),
            "diagnostics",
        ),
    ] {
        if value == 0 || value > maximum {
            return Err(ProtocolObservationError::BoundExceeded(field));
        }
    }
    let participants = u32::try_from(profile.producers.len())
        .map_err(|_| ProtocolObservationError::CardinalityOverflow)?;
    if participants > bounds.max_participants
        || participants > bounds.max_cohort_backlog
        || participants > bounds.max_oracle_work_items
        || bounds.max_total_projection_bytes < u64::from(bounds.max_projection_bytes)
    {
        return Err(ProtocolObservationError::BoundExceeded(
            "inconsistent-bounds",
        ));
    }
    Ok(())
}
