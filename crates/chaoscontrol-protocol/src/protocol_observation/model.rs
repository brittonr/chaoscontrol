use super::oracle::{MarkerSnapshotBinding, ProtocolOracleResult, ProtocolVerdict};
use serde::{Deserialize, Serialize};

pub const PROFILE_SCHEMA: &str = "chaoscontrol.protocol-observation-profile.v1";
pub const DRAFT_SCHEMA: &str = "chaoscontrol.protocol-observation-draft.v1";
pub const COLLECTED_SCHEMA: &str = "chaoscontrol.protocol-observation-collected.v1";
pub const RECEIPT_SCHEMA: &str = "chaoscontrol.protocol-observation-receipt.v1";
pub const PROTOCOL_OBSERVATION_EVENT: &str = "chaoscontrol_protocol_observation";
pub const MAX_PROFILE_PARTICIPANTS: usize = 256;
pub const MAX_PROFILE_PRODUCERS: usize = 256;
pub const MAX_NOVELTY_SELECTORS: usize = 32;
pub const MAX_NON_CLAIMS: usize = 32;
pub const MAX_REFERENCE_BYTES: usize = 128;
pub const MAX_TRANSITION_CLASS_BYTES: usize = 128;
pub const MAX_INLINE_PROJECTION_BYTES: usize = 2_048;
pub const MAX_ACTIVE_VCPUS: u32 = 256;
pub const BLAKE3_HEX_BYTES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OracleAuthority {
    ConsumerIndependent,
    RuntimeSelfReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CompletionRule {
    AllRequiredParticipants,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MarkerPolicy {
    Denied,
    OptionalDeclared,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NoveltySelector {
    ProjectionRef,
    LogicalBoundaryRef,
    TransitionClass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectionSupport {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DrainState {
    Open,
    Final,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolObservationBounds {
    pub max_records_per_producer: u32,
    pub max_projection_bytes: u32,
    pub max_total_projection_bytes: u64,
    pub max_participants: u32,
    pub max_logical_boundaries: u32,
    pub max_cohort_backlog: u32,
    pub max_oracle_work_items: u32,
    pub max_diagnostic_refs: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerProfile {
    pub producer_ref: String,
    pub participant_ref: String,
    pub process_ref: String,
    pub vm_id: u32,
    pub admitted_generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAdapterProfile {
    pub adapter_ref: String,
    pub authority: OracleAuthority,
    pub requires_complete_cohort: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolObservationProfile {
    pub schema: String,
    pub execution_ref: String,
    pub protocol_ref: String,
    pub projection_schema_ref: String,
    pub logical_boundary_schema_ref: String,
    pub cohort_ref: String,
    pub producers: Vec<ProducerProfile>,
    pub required_participants: Vec<String>,
    pub completion_rule: CompletionRule,
    pub oracle: OracleAdapterProfile,
    pub novelty_selectors: Vec<NoveltySelector>,
    pub marker_policy: MarkerPolicy,
    pub bounds: ProtocolObservationBounds,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedProfile {
    pub profile_ref: String,
    pub bounds_ref: String,
    pub profile: ProtocolObservationProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationDraft {
    pub schema: String,
    pub profile_ref: String,
    pub protocol_ref: String,
    pub cohort_ref: String,
    pub producer_ref: String,
    pub participant_ref: String,
    pub process_ref: String,
    pub execution_ref: String,
    pub generation: u64,
    pub source_sequence: u64,
    pub source_loss_count: u64,
    pub drain_state: DrainState,
    pub transition_class: String,
    pub logical_boundary_ref: String,
    pub projection_schema_ref: String,
    pub projection_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projection_bytes: Option<Vec<u8>>,
    pub novelty_identity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker_identity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_snapshot_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchedulerPosition {
    pub schedule_state_ref: String,
    pub vm_id: u32,
    pub active_vcpu: u32,
    pub guest_exit_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectedObservation {
    pub schema: String,
    pub draft: ObservationDraft,
    pub scheduler_position: SchedulerPosition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmittedObservation {
    pub record_identity: String,
    pub collected: CollectedObservation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CohortClassification {
    Complete,
    Incomplete,
    Conflicting,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CohortIssueKind {
    BoundExceeded,
    ConflictingProjection,
    DuplicateSequence,
    FailedFinalDrain,
    PostFinalRecord,
    GenerationDrift,
    IdentityDrift,
    LossObserved,
    MissingParticipant,
    SequenceGap,
    UnsupportedProjection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CohortIssue {
    pub kind: CohortIssueKind,
    pub subject: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CohortResult {
    pub cohort_identity: String,
    pub profile_ref: String,
    pub projection_support: ProjectionSupport,
    pub source_records: Vec<CollectedObservation>,
    pub host_loss_count: u64,
    pub cohort_ref: String,
    pub logical_boundary_ref: String,
    pub classification: CohortClassification,
    pub records: Vec<AdmittedObservation>,
    pub issues: Vec<CohortIssue>,
    pub novelty_identities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolEvidenceContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker_binding: Option<MarkerSnapshotBinding>,
    pub fault_refs: Vec<String>,
    pub replay_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolObservationReceipt {
    pub schema: String,
    pub receipt_ref: String,
    pub profile_ref: String,
    pub bounds_ref: String,
    pub protocol_ref: String,
    pub projection_schema_ref: String,
    pub producer_refs: Vec<String>,
    pub participant_refs: Vec<String>,
    pub record_refs: Vec<String>,
    pub cohort_identity: String,
    pub logical_boundary_ref: String,
    pub classification: CohortClassification,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oracle_result: Option<ProtocolOracleResult>,
    pub novelty_identities: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker_binding: Option<MarkerSnapshotBinding>,
    pub scheduler_state_refs: Vec<String>,
    pub fault_refs: Vec<String>,
    pub replay_refs: Vec<String>,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MarkerReachability {
    NotBound,
    /// The identities match. The snapshot shell still owns reachability evidence.
    IdentityLinked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolObservationStatus {
    pub classification: CohortClassification,
    pub required_participants: usize,
    pub observed_participants: usize,
    pub missing_participants: Vec<String>,
    pub sequence_gap_count: usize,
    pub loss_count: usize,
    pub conflict_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oracle_verdict: Option<ProtocolVerdict>,
    pub novelty_count: usize,
    pub marker_reachability: MarkerReachability,
    pub blocker_kinds: Vec<CohortIssueKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolObservationError {
    BoundExceeded(&'static str),
    CardinalityOverflow,
    ClaimOverreach,
    CohortNotComplete,
    DuplicateParticipant,
    DuplicateProducer,
    EmptyField(&'static str),
    IdentityMismatch(&'static str),
    InvalidCanonicalProjection,
    InvalidReference(&'static str),
    InvalidSchema,
    MarkerMismatch,
    NonCanonicalOrder(&'static str),
    OracleMismatch,
    OracleWorkExceeded,
    RuntimeSelfOracle,
    UnknownParticipant,
    UnknownProducer,
}
