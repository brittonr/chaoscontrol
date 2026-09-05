//! Bounded protocol-observation envelopes, cohorts, and evidence.
//!
//! This module owns structural admission and deterministic identity only.
//! Consumers retain protocol meaning and oracle authority.

mod admission;
mod cohort;
mod evidence;
mod model;
mod oracle;
mod profile;

pub use admission::{
    bind_scheduler_position, validate_scheduler_position, validate_transport_draft,
};
pub use cohort::{assemble_with_losses, bounded_record};
pub use evidence::{build_receipt, build_status, validate_claim, validate_receipt};
pub use model::{
    AdmittedProfile, CohortClassification, CohortIssue, CohortIssueKind, CohortResult,
    CompletionRule, DrainState, MarkerPolicy, MarkerReachability, NoveltySelector,
    OracleAdapterProfile, OracleAuthority, ProducerProfile, ProjectionSupport, SchedulerPosition,
    BLAKE3_HEX_BYTES, COLLECTED_SCHEMA, DRAFT_SCHEMA, MAX_ACTIVE_VCPUS,
    MAX_INLINE_PROJECTION_BYTES, MAX_NON_CLAIMS, MAX_NOVELTY_SELECTORS, MAX_PROFILE_PARTICIPANTS,
    MAX_PROFILE_PRODUCERS, MAX_REFERENCE_BYTES, MAX_TRANSITION_CLASS_BYTES, PROFILE_SCHEMA,
    PROTOCOL_OBSERVATION_EVENT, RECEIPT_SCHEMA,
};
pub use oracle::ProtocolOracle;
pub use profile::{MAX_COHORT_RECORDS, MAX_PROFILE_BYTES, REQUIRED_NON_CLAIMS};

// Compatibility: preserve the existing root admission entry point.
pub use admission::admit as admit_observation;
// Compatibility: preserve the existing collected-record validator.
pub use admission::validate_collected as validate_collected_observation;
// Compatibility: preserve the existing draft validator.
pub use admission::validate_draft as validate_observation_draft;
// Compatibility: preserve the existing cohort assembly entry point.
pub use cohort::assemble as assemble_cohort;
// Compatibility: preserve the existing cohort validator.
pub use cohort::validate as validate_cohort;
// Compatibility: preserve the existing claim type.
pub use evidence::Claim as ProtocolObservationClaim;
// Compatibility: preserve the existing admitted-record type.
pub use model::Admitted as AdmittedObservation;
// Compatibility: preserve the existing bound type.
pub use model::Bounds as ProtocolObservationBounds;
// Compatibility: preserve the existing collected-record type.
pub use model::Collected as CollectedObservation;
// Compatibility: preserve the existing draft type.
pub use model::Draft as ObservationDraft;
// Compatibility: preserve the existing error type and its variants.
pub use model::Error as ProtocolObservationError;
// Compatibility: preserve the existing evidence context.
pub use model::EvidenceContext as ProtocolEvidenceContext;
// Compatibility: preserve the existing profile type.
pub use model::Profile as ProtocolObservationProfile;
// Compatibility: preserve the existing receipt type.
pub use model::Receipt as ProtocolObservationReceipt;
// Compatibility: preserve the existing status type.
pub use model::Status as ProtocolObservationStatus;
// Compatibility: preserve the existing snapshot binding entry point.
pub use oracle::bind_snapshot as bind_marker_snapshot;
// Compatibility: preserve the existing consumer adapter entry point.
pub use oracle::run_consumer as run_consumer_oracle;
// Compatibility: preserve the existing adapter validator.
pub use oracle::validate_adapter as validate_oracle_adapter;
// Compatibility: preserve the existing marker validator.
pub use oracle::validate_binding as validate_marker_binding;
// Compatibility: preserve the existing result validator.
pub use oracle::validate_result as validate_oracle_result;
// Compatibility: preserve the existing snapshot binding type.
pub use oracle::Binding as MarkerSnapshotBinding;
// Compatibility: preserve the existing consumer decision type.
pub use oracle::Decision as OracleDecision;
// Compatibility: preserve the existing result type.
pub use oracle::Outcome as ProtocolOracleResult;
// Compatibility: preserve the existing verdict type and its variants.
pub use oracle::Verdict as ProtocolVerdict;
// Compatibility: preserve the existing profile admission entry point.
pub use profile::admit as admit_profile;
// Compatibility: preserve the existing profile decoder.
pub use profile::decode as decode_profile;
// Compatibility: preserve the existing profile identity validator.
pub use profile::validate_identity as validate_profile_identity;

const PROFILE_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.protocol-observation.profile.v1\0";
const BOUNDS_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.protocol-observation.bounds.v1\0";
const PROJECTION_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.protocol-observation.projection.v1\0";
const NOVELTY_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.protocol-observation.novelty.v1\0";
const RECORD_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.protocol-observation.record.v1\0";
const COHORT_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.protocol-observation.cohort.v1\0";
const NOVELTY_IDENTITY_PREFIX: &str = "protocol-novelty";
const NOVELTY_SLOT_HEX_BYTES: usize = 16;
const HEX_RADIX: u32 = 16;

pub fn projection_identity(bytes: &[u8]) -> String {
    digest_reference("projection", PROJECTION_IDENTITY_DOMAIN, bytes)
}

pub fn novelty_identity(
    profile: &AdmittedProfile,
    projection_ref: &str,
    logical_boundary_ref: &str,
    transition_class: &str,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(NOVELTY_IDENTITY_DOMAIN);
    hash_field(&mut hasher, profile.profile_ref.as_bytes());
    for selector in &profile.profile.novelty_selectors {
        match selector {
            NoveltySelector::ProjectionRef => hash_field(&mut hasher, projection_ref.as_bytes()),
            NoveltySelector::LogicalBoundaryRef => {
                hash_field(&mut hasher, logical_boundary_ref.as_bytes());
            }
            NoveltySelector::TransitionClass => {
                hash_field(&mut hasher, transition_class.as_bytes());
            }
        }
    }
    format!("protocol-novelty:{}", hasher.finalize().to_hex())
}

pub fn novelty_coverage_slot(
    novelty_identity: &str,
    region_start: usize,
    region_size: usize,
) -> Result<usize, ProtocolObservationError> {
    if region_size == 0 {
        return Err(ProtocolObservationError::BoundExceeded("coverage-region"));
    }
    validate_exact_reference(novelty_identity, NOVELTY_IDENTITY_PREFIX)?;
    let hex = novelty_identity
        .rsplit_once(':')
        .map(|(_, hex)| hex)
        .ok_or(ProtocolObservationError::InvalidReference(
            NOVELTY_IDENTITY_PREFIX,
        ))?;
    let prefix =
        hex.get(..NOVELTY_SLOT_HEX_BYTES)
            .ok_or(ProtocolObservationError::InvalidReference(
                NOVELTY_IDENTITY_PREFIX,
            ))?;
    let value = u64::from_str_radix(prefix, HEX_RADIX)
        .map_err(|_| ProtocolObservationError::InvalidReference(NOVELTY_IDENTITY_PREFIX))?;
    let size =
        u64::try_from(region_size).map_err(|_| ProtocolObservationError::CardinalityOverflow)?;
    let offset =
        usize::try_from(value % size).map_err(|_| ProtocolObservationError::CardinalityOverflow)?;
    region_start
        .checked_add(offset)
        .ok_or(ProtocolObservationError::CardinalityOverflow)
}

pub fn validate_exact_reference(
    value: &str,
    prefix: &'static str,
) -> Result<(), ProtocolObservationError> {
    if value.len() > MAX_REFERENCE_BYTES {
        return Err(ProtocolObservationError::InvalidReference(prefix));
    }
    let Some(hex) = value
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix(':'))
    else {
        return Err(ProtocolObservationError::InvalidReference(prefix));
    };
    if hex.len() != BLAKE3_HEX_BYTES
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ProtocolObservationError::InvalidReference(prefix));
    }
    Ok(())
}

pub(super) fn digest_reference(prefix: &str, domain: &[u8], bytes: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hash_field(&mut hasher, bytes);
    format!("{prefix}:{}", hasher.finalize().to_hex())
}

pub(super) fn hash_field(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    let length = u64::try_from(bytes.len()).expect("bounded identity fields fit a u64 length");
    hasher.update(&length.to_le_bytes());
    hasher.update(bytes);
}
