//! Source-bound opaque observations; protocol semantics remain consumer-owned.
use crate::{canonical_value_ref, validate_digest_ref, SmrError, SmrResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub mod cohort;
pub mod guest;
pub use cohort::{assemble_cohort, CohortResult, Completeness, Drain};

pub const SCHEMA: &str = "chaoscontrol.protocol-observation.v1";
pub const PROFILE_SCHEMA: &str = "chaoscontrol.protocol-observation-profile.v1";
pub const MAX_PARTICIPANTS: usize = 4096;
pub const MAX_RECORDS: u64 = 1_000_000;
pub const MAX_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Participant {
    pub participant_ref: String,
    pub producer_ref: String,
    pub guest_id: u32,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bounds {
    pub records: u64,
    pub record_bytes: u64,
    pub total_bytes: u64,
    pub boundaries: u64,
    pub cohort_backlog: u64,
    pub oracle_work: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolProfile {
    pub schema: String,
    pub protocol_ref: String,
    pub projection_schema_ref: String,
    pub boundary_schema_ref: String,
    pub oracle_adapter_ref: String,
    pub independent_oracle_ref: String,
    pub novelty_selection_ref: String,
    pub participants: Vec<Participant>,
    pub bounds: Bounds,
    pub marker_policy: MarkerPolicy,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MarkerPolicy {
    Disabled,
}

pub const NON_CLAIMS: [&str; 5] = [
    "universal-protocol-correctness",
    "production-readiness",
    "release-eligibility",
    "snapshot-backed-replay",
    "cross-participant-total-order",
];

impl ProtocolProfile {
    // r[impl chaoscontrol.protocol_observation.profile]
    pub fn admit(&self) -> SmrResult<String> {
        if self.schema != PROFILE_SCHEMA {
            return Err(error("unsupported", "unknown profile schema"));
        }
        for value in [
            &self.protocol_ref,
            &self.projection_schema_ref,
            &self.boundary_schema_ref,
            &self.oracle_adapter_ref,
            &self.independent_oracle_ref,
            &self.novelty_selection_ref,
        ] {
            validate_digest_ref("profile identity", value)?;
        }
        if self.oracle_adapter_ref == self.independent_oracle_ref {
            return Err(error(
                "unsupported",
                "runtime self-oracle is not independent",
            ));
        }
        if self.participants.is_empty() || self.participants.len() > MAX_PARTICIPANTS {
            return Err(error("bound", "participant count outside supported bounds"));
        }
        let mut participants = BTreeSet::new();
        let mut producers = BTreeSet::new();
        for participant in &self.participants {
            validate_digest_ref("participant", &participant.participant_ref)?;
            validate_digest_ref("producer", &participant.producer_ref)?;
            if !participants.insert(&participant.participant_ref)
                || !producers.insert(&participant.producer_ref)
            {
                return Err(error("profile", "duplicate participant or producer"));
            }
        }
        self.bounds.validate()?;
        if NON_CLAIMS
            .iter()
            .any(|claim| !self.non_claims.iter().any(|v| v == claim))
        {
            return Err(error("claim", "required non-claim missing"));
        }
        canonical_value_ref(b"chaoscontrol.protocol-profile.v1\0", self)
    }
}

impl Bounds {
    fn validate(&self) -> SmrResult<()> {
        for value in [
            self.records,
            self.boundaries,
            self.cohort_backlog,
            self.oracle_work,
        ] {
            if value == 0 || value > MAX_RECORDS {
                return Err(error("bound", "invalid finite record or work bound"));
            }
        }
        if self.record_bytes == 0
            || self.record_bytes > self.total_bytes
            || self.total_bytes > MAX_BYTES
            || self.boundaries > self.records
            || self.cohort_backlog > self.boundaries
        {
            return Err(error("bound", "incoherent byte or cohort bounds"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolObservation {
    pub schema: String,
    pub profile_ref: String,
    pub protocol_ref: String,
    pub projection_schema_ref: String,
    pub participant_ref: String,
    pub producer_ref: String,
    pub guest_id: u32,
    pub generation: u64,
    pub source_sequence: u64,
    pub transition_class_ref: String,
    pub boundary_ref: String,
    pub projection_ref: String,
    pub projection: Option<Vec<u8>>,
    pub scheduler_ref: String,
    pub dropped: u64,
}

impl ProtocolObservation {
    // r[impl chaoscontrol.protocol_observation.envelope]
    pub fn admit(&self, profile: &ProtocolProfile, profile_ref: &str) -> SmrResult<String> {
        if self.schema != SCHEMA
            || self.profile_ref != profile_ref
            || self.protocol_ref != profile.protocol_ref
            || self.projection_schema_ref != profile.projection_schema_ref
        {
            return Err(error(
                "unsupported",
                "profile or projection schema mismatch",
            ));
        }
        for value in [
            &self.transition_class_ref,
            &self.boundary_ref,
            &self.projection_ref,
            &self.scheduler_ref,
        ] {
            validate_digest_ref("observation identity", value)?;
        }
        let participant = profile
            .participants
            .iter()
            .find(|p| p.participant_ref == self.participant_ref)
            .ok_or_else(|| error("unsupported", "unknown participant"))?;
        if self.producer_ref != participant.producer_ref
            || self.guest_id != participant.guest_id
            || self.generation != participant.generation
            || self.source_sequence == 0
        {
            return Err(error(
                "unsupported",
                "source identity or generation mismatch",
            ));
        }
        if let Some(bytes) = &self.projection {
            if projection_ref(bytes) != self.projection_ref {
                return Err(error("conflicting", "projection digest mismatch"));
            }
        }
        let bytes = serde_json::to_vec(self).map_err(|e| error("malformed", e.to_string()))?;
        if bytes.len() as u64 > profile.bounds.record_bytes {
            return Err(error("bound", "record byte bound exceeded"));
        }
        canonical_value_ref(b"chaoscontrol.protocol-observation.v1\0", self)
    }
}

pub fn projection_ref(bytes: &[u8]) -> String {
    let mut hash = blake3::Hasher::new();
    hash.update(b"chaoscontrol.protocol-projection.v1\0");
    hash.update(&(bytes.len() as u64).to_be_bytes());
    hash.update(bytes);
    format!("blake3:{}", hash.finalize().to_hex())
}

pub(super) fn error(class: &'static str, detail: impl Into<String>) -> SmrError {
    SmrError::new(class, detail)
}
