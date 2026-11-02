//! Marker-linked protocol evidence. A marker or snapshot never completes a
//! cohort and never claims one cross-participant wall-clock instant.
use super::{
    error, CohortResult, Completeness, MarkerPolicy, ProtocolObservation, ProtocolProfile,
};
use crate::{canonical_value_ref, validate_digest_ref, SmrResult};
use serde::{Deserialize, Serialize};

pub const MARKER_BINDING_SCHEMA: &str = "chaoscontrol.protocol-marker-binding.v1";

/// Declared-marker binding for one admitted observation at one logical boundary.
///
/// The retained cohort completeness is the cohort's own result; binding cannot
/// upgrade it. A restorable parent snapshot reference identifies retained state
/// for replay and does not establish that every participant was observed at one
/// wall-clock instant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkerBinding {
    pub schema: String,
    pub marker_ref: String,
    pub boundary_ref: String,
    pub projection_ref: String,
    pub cohort_ref: String,
    pub parent_snapshot_ref: String,
    pub cohort_completeness: Completeness,
}

impl MarkerBinding {
    /// Bind a declared marker to one admitted observation and cohort.
    ///
    /// # Errors
    ///
    /// Rejects a profile that does not admit declared markers, malformed marker
    /// or snapshot references, an observation from another profile or logical
    /// boundary, and a cohort identity that does not match the profile.
    // r[impl chaoscontrol.protocol_observation.snapshot_binding]
    pub fn bind(
        profile: &ProtocolProfile,
        marker_ref: &str,
        record: &ProtocolObservation,
        cohort: &CohortResult,
        parent_snapshot_ref: &str,
    ) -> SmrResult<Self> {
        let profile_ref = profile.admit()?;
        if profile.marker_policy != MarkerPolicy::Declared {
            return Err(error(
                "unsupported",
                "profile does not admit declared marker bindings",
            ));
        }
        validate_digest_ref("declared marker", marker_ref)?;
        validate_digest_ref("restorable parent snapshot", parent_snapshot_ref)?;
        record.admit(profile, &profile_ref)?;
        if cohort.profile_ref != profile_ref {
            return Err(error("conflicting", "marker cohort profile differs"));
        }
        if record.boundary_ref != cohort.boundary_ref {
            return Err(error(
                "conflicting",
                "marker observation is outside the cohort boundary",
            ));
        }
        if cohort.record_refs.is_empty() {
            return Err(error(
                "incomplete",
                "marker cohort carries no admitted record",
            ));
        }
        Ok(Self {
            schema: MARKER_BINDING_SCHEMA.into(),
            marker_ref: marker_ref.into(),
            boundary_ref: record.boundary_ref.clone(),
            projection_ref: record.projection_ref.clone(),
            cohort_ref: cohort.cohort_ref.clone(),
            parent_snapshot_ref: parent_snapshot_ref.into(),
            cohort_completeness: cohort.completeness,
        })
    }

    /// Revalidate a retained binding against the same profile and cohort.
    ///
    /// # Errors
    ///
    /// Rejects schema drift, a completeness upgrade, and any changed boundary,
    /// projection, or cohort identity.
    // r[impl chaoscontrol.protocol_observation.snapshot_binding]
    pub fn admit(
        &self,
        profile: &ProtocolProfile,
        record: &ProtocolObservation,
        cohort: &CohortResult,
    ) -> SmrResult<String> {
        let profile_ref = profile.admit()?;
        if self.schema != MARKER_BINDING_SCHEMA || profile.marker_policy != MarkerPolicy::Declared {
            return Err(error(
                "unsupported",
                "marker binding schema or policy differs",
            ));
        }
        validate_digest_ref("declared marker", &self.marker_ref)?;
        validate_digest_ref("restorable parent snapshot", &self.parent_snapshot_ref)?;
        record.admit(profile, &profile_ref)?;
        if cohort.profile_ref != profile_ref
            || self.cohort_ref != cohort.cohort_ref
            || self.cohort_completeness != cohort.completeness
        {
            return Err(error(
                "conflicting",
                "marker binding cohort identity or completeness drifted",
            ));
        }
        if self.boundary_ref != cohort.boundary_ref
            || self.boundary_ref != record.boundary_ref
            || self.projection_ref != record.projection_ref
        {
            return Err(error(
                "conflicting",
                "marker binding boundary or projection drifted",
            ));
        }
        canonical_value_ref(b"chaoscontrol.protocol-marker-binding.v1\0", self)
    }

    /// Whether this binding may support complete protocol-state coverage.
    ///
    /// Only an already complete cohort can do so; binding never upgrades an
    /// incomplete cohort.
    #[must_use]
    pub fn complete_protocol_state(&self) -> bool {
        self.cohort_completeness == Completeness::Complete
    }
}
