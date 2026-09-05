//! Bounded envelope admission without protocol interpretation.

use super::*;

pub fn bind_scheduler_position(
    draft: ObservationDraft,
    scheduler_position: SchedulerPosition,
) -> Result<CollectedObservation, ProtocolObservationError> {
    validate_scheduler_position(&scheduler_position)?;
    validate_transport_draft(&draft)?;
    Ok(CollectedObservation {
        schema: COLLECTED_SCHEMA.to_string(),
        draft,
        scheduler_position,
    })
}

pub fn admit_observation(
    profile: &AdmittedProfile,
    collected: CollectedObservation,
) -> Result<AdmittedObservation, ProtocolObservationError> {
    validate_collected_observation(profile, &collected)?;
    let bytes =
        serde_json::to_vec(&collected).map_err(|_| ProtocolObservationError::InvalidSchema)?;
    Ok(AdmittedObservation {
        record_identity: digest_reference("protocol-record", RECORD_IDENTITY_DOMAIN, &bytes),
        collected,
    })
}

pub fn validate_collected_observation(
    profile: &AdmittedProfile,
    collected: &CollectedObservation,
) -> Result<(), ProtocolObservationError> {
    if collected.schema != COLLECTED_SCHEMA {
        return Err(ProtocolObservationError::InvalidSchema);
    }
    validate_scheduler_position(&collected.scheduler_position)?;
    validate_observation_draft(profile, &collected.draft)?;
    let producer = profile
        .profile
        .producers
        .iter()
        .find(|producer| producer.producer_ref == collected.draft.producer_ref)
        .ok_or(ProtocolObservationError::UnknownProducer)?;
    if producer.vm_id != collected.scheduler_position.vm_id {
        return Err(ProtocolObservationError::IdentityMismatch("vm"));
    }
    Ok(())
}

pub fn validate_observation_draft(
    profile: &AdmittedProfile,
    draft: &ObservationDraft,
) -> Result<(), ProtocolObservationError> {
    validate_profile_identity(profile)?;
    validate_transport_draft(draft)?;
    if draft.profile_ref != profile.profile_ref
        || draft.protocol_ref != profile.profile.protocol_ref
        || draft.cohort_ref != profile.profile.cohort_ref
        || draft.execution_ref != profile.profile.execution_ref
        || draft.projection_schema_ref != profile.profile.projection_schema_ref
    {
        return Err(ProtocolObservationError::IdentityMismatch(
            "profile-binding",
        ));
    }
    let producer = profile
        .profile
        .producers
        .iter()
        .find(|producer| producer.producer_ref == draft.producer_ref)
        .ok_or(ProtocolObservationError::UnknownProducer)?;
    if producer.participant_ref != draft.participant_ref {
        return Err(ProtocolObservationError::UnknownParticipant);
    }
    if producer.process_ref != draft.process_ref {
        return Err(ProtocolObservationError::IdentityMismatch("process"));
    }
    if producer.admitted_generation != draft.generation {
        return Err(ProtocolObservationError::IdentityMismatch("generation"));
    }
    if draft
        .projection_bytes
        .as_ref()
        .is_some_and(|bytes| bytes.len() > profile.profile.bounds.max_projection_bytes as usize)
    {
        return Err(ProtocolObservationError::BoundExceeded("projection-bytes"));
    }
    let expected = novelty_identity(
        profile,
        &draft.projection_ref,
        &draft.logical_boundary_ref,
        &draft.transition_class,
    );
    if draft.novelty_identity != expected {
        return Err(ProtocolObservationError::IdentityMismatch("novelty"));
    }
    if profile.profile.marker_policy == MarkerPolicy::Denied
        && (draft.marker_identity.is_some() || draft.parent_snapshot_ref.is_some())
    {
        return Err(ProtocolObservationError::InvalidReference("marker-policy"));
    }
    Ok(())
}

/// Validate transport shape before host retention. This does not admit a profile.
pub fn validate_transport_draft(draft: &ObservationDraft) -> Result<(), ProtocolObservationError> {
    if draft.schema != DRAFT_SCHEMA {
        return Err(ProtocolObservationError::InvalidSchema);
    }
    for (value, prefix) in [
        (&draft.profile_ref, "protocol-observation-profile"),
        (&draft.protocol_ref, "protocol"),
        (&draft.cohort_ref, "cohort"),
        (&draft.execution_ref, "execution"),
        (&draft.producer_ref, "producer"),
        (&draft.participant_ref, "participant"),
        (&draft.process_ref, "process"),
        (&draft.logical_boundary_ref, "logical-boundary"),
        (&draft.projection_schema_ref, "projection-schema"),
        (&draft.projection_ref, "projection"),
        (&draft.novelty_identity, "protocol-novelty"),
    ] {
        validate_exact_reference(value, prefix)?;
    }
    if draft.source_sequence == u64::MAX || draft.source_loss_count > draft.source_sequence {
        return Err(ProtocolObservationError::CardinalityOverflow);
    }
    if draft.transition_class.is_empty()
        || draft.transition_class.len() > MAX_TRANSITION_CLASS_BYTES
        || draft.transition_class.chars().any(char::is_control)
    {
        return Err(ProtocolObservationError::EmptyField("transition-class"));
    }
    if let Some(bytes) = &draft.projection_bytes {
        if bytes.len() > MAX_INLINE_PROJECTION_BYTES {
            return Err(ProtocolObservationError::BoundExceeded("projection-bytes"));
        }
        let mut value: serde_json::Value = serde_json::from_slice(bytes)
            .map_err(|_| ProtocolObservationError::InvalidCanonicalProjection)?;
        value.sort_all_objects();
        let canonical = serde_json::to_vec(&value)
            .map_err(|_| ProtocolObservationError::InvalidCanonicalProjection)?;
        if canonical != *bytes || draft.projection_ref != projection_identity(bytes) {
            return Err(ProtocolObservationError::InvalidCanonicalProjection);
        }
    }
    if let Some(marker) = &draft.marker_identity {
        validate_exact_reference(marker, "b3")?;
    }
    if let Some(snapshot) = &draft.parent_snapshot_ref {
        if draft.marker_identity.is_none() {
            return Err(ProtocolObservationError::InvalidReference(
                "snapshot-without-marker",
            ));
        }
        validate_exact_reference(snapshot, "snapshot")?;
    }
    Ok(())
}

pub fn validate_scheduler_position(
    position: &SchedulerPosition,
) -> Result<(), ProtocolObservationError> {
    validate_exact_reference(&position.schedule_state_ref, "schedule-state")?;
    if position.active_vcpu >= MAX_ACTIVE_VCPUS {
        return Err(ProtocolObservationError::BoundExceeded("active-vcpu"));
    }
    Ok(())
}
