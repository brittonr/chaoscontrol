use super::*;
use crate::branch_marker::BranchMarker;

/// Identity linkage only. The snapshot shell must establish restorability.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkerSnapshotBinding {
    pub marker_identity: String,
    pub projection_ref: String,
    pub record_ref: String,
    pub cohort_identity: String,
    pub logical_boundary_ref: String,
    pub parent_snapshot_ref: String,
    pub scheduler_state_ref: String,
}

pub fn bind_marker_snapshot(
    profile: &AdmittedProfile,
    cohort: &CohortResult,
    marker: &BranchMarker,
    projection_ref: &str,
    parent_snapshot_ref: &str,
) -> Result<MarkerSnapshotBinding, ProtocolObservationError> {
    validate_cohort(profile, cohort)?;
    marker
        .validate()
        .map_err(|_| ProtocolObservationError::MarkerMismatch)?;
    if marker.logical_position_ref.as_deref() != Some(cohort.logical_boundary_ref.as_str())
        || marker
            .details
            .get("projection_ref")
            .and_then(serde_json::Value::as_str)
            != Some(projection_ref)
    {
        return Err(ProtocolObservationError::MarkerMismatch);
    }
    let record = cohort
        .records
        .iter()
        .find(|record| {
            record.collected.draft.projection_ref == projection_ref
                && record.collected.draft.marker_identity.as_deref()
                    == Some(marker.identity.as_str())
        })
        .ok_or(ProtocolObservationError::MarkerMismatch)?;
    let binding = MarkerSnapshotBinding {
        marker_identity: marker.identity.clone(),
        projection_ref: projection_ref.to_string(),
        record_ref: record.record_identity.clone(),
        cohort_identity: cohort.cohort_identity.clone(),
        logical_boundary_ref: cohort.logical_boundary_ref.clone(),
        parent_snapshot_ref: parent_snapshot_ref.to_string(),
        scheduler_state_ref: record
            .collected
            .scheduler_position
            .schedule_state_ref
            .clone(),
    };
    validate_marker_binding(profile, cohort, &binding)?;
    Ok(binding)
}

pub fn validate_marker_binding(
    profile: &AdmittedProfile,
    cohort: &CohortResult,
    binding: &MarkerSnapshotBinding,
) -> Result<(), ProtocolObservationError> {
    validate_cohort(profile, cohort)?;
    if profile.profile.marker_policy != MarkerPolicy::OptionalDeclared
        || cohort.classification != CohortClassification::Complete
        || binding.cohort_identity != cohort.cohort_identity
        || binding.logical_boundary_ref != cohort.logical_boundary_ref
    {
        return Err(ProtocolObservationError::MarkerMismatch);
    }
    validate_exact_reference(&binding.parent_snapshot_ref, "snapshot")?;
    let record = cohort
        .records
        .iter()
        .find(|record| record.record_identity == binding.record_ref)
        .ok_or(ProtocolObservationError::MarkerMismatch)?;
    let draft = &record.collected.draft;
    if draft.marker_identity.as_deref() != Some(binding.marker_identity.as_str())
        || draft.projection_ref != binding.projection_ref
        || draft
            .parent_snapshot_ref
            .as_ref()
            .is_some_and(|reference| reference != &binding.parent_snapshot_ref)
        || record.collected.scheduler_position.schedule_state_ref != binding.scheduler_state_ref
    {
        return Err(ProtocolObservationError::MarkerMismatch);
    }
    Ok(())
}
