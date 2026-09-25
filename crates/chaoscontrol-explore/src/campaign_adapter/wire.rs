//! Product-owned serde/CBOR mirror of Campaign control events and moment drafts.
//!
//! `campaign-core` has no serde. The journal stores these mirror values, and every
//! decode re-derives Campaign and Choregraph identities: the control event identity,
//! the Choregraph control-payload identity, the admission identity, and the moment
//! event identity. Codec drift or a rewritten record therefore fails as an identity
//! mismatch instead of producing another decision.

use campaign_choregraph::{
    derive_control_payload_id, Digest, EventClass, EventDraft, EventId, HistoryGraph,
    MetadataIdentity, ParentLink,
};
use campaign_core::{
    AdmissionDecision, AdmissionOutcome, AdmissionRule, CampaignAdmission, CampaignControlEvent,
    CandidateBatchEvent, CandidateRecord, ChildObservation, ClosureEvent, DuplicateCandidateRule,
    EntropyTicket, ExplorationBounds, FindingRef, GuidanceDynamics, GuidanceEvent, GuidanceRecord,
    Id, IdKind, PolicyImplementation, ProjectionBinding, PruneEvent, PublicationPlan,
    ResourceAmount, ResultEvent, ResultObservation, RetirementEvent, RetirementOrigin, RootMoment,
    SelectionCountDecay, SelectionDecision, SelectionEvent, SelectionMode, SelectionProfile,
    StopDecision, StopEvent, StopReason, StoppingRule, UnobservedClosure, DIGEST_LENGTH,
};
use serde::{Deserialize, Serialize};

/// Journal mirror failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WireError {
    /// CBOR encoding failed.
    Encode,
    /// CBOR decoding failed or bytes follow the record.
    Decode,
    /// The record names an unknown identity kind code.
    UnknownIdKind(u8),
    /// The record names an unknown enumeration code.
    UnknownCode(u32),
    /// A re-derived Campaign or Choregraph identity differs from the recorded one.
    IdentityMismatch,
    /// Campaign rejected a carried identity while re-deriving.
    Campaign(campaign_core::CampaignError),
}

impl From<campaign_core::CampaignError> for WireError {
    fn from(error: campaign_core::CampaignError) -> Self {
        Self::Campaign(error)
    }
}

type WireResult<T> = Result<T, WireError>;

const ID_KINDS: [IdKind; 48] = [
    IdKind::Absent,
    IdKind::Campaign,
    IdKind::Projection,
    IdKind::Graph,
    IdKind::ControlBranch,
    IdKind::Head,
    IdKind::Policy,
    IdKind::GuidanceProfile,
    IdKind::EntropyProfile,
    IdKind::BudgetProfile,
    IdKind::Adapter,
    IdKind::ControlDomain,
    IdKind::RootMoment,
    IdKind::OperationProfile,
    IdKind::OperationSchema,
    IdKind::CostProfile,
    IdKind::Candidate,
    IdKind::GuidanceRecord,
    IdKind::State,
    IdKind::Selection,
    IdKind::Operation,
    IdKind::Event,
    IdKind::Result,
    IdKind::ChildReservation,
    IdKind::ChildMoment,
    IdKind::Finding,
    IdKind::Reason,
    IdKind::EntropyTicket,
    IdKind::Receipt,
    IdKind::Cache,
    IdKind::SchedulingProfile,
    IdKind::AdmissionPolicy,
    IdKind::CoalescingPolicy,
    IdKind::SourceFacts,
    IdKind::EventPayload,
    IdKind::PrunePlan,
    IdKind::Stop,
    IdKind::Discovery,
    IdKind::ResourceDimension,
    IdKind::SelectionProfile,
    IdKind::Request,
    IdKind::DerivedRecord,
    IdKind::Relaxation,
    IdKind::Invalidation,
    IdKind::RetentionProfile,
    IdKind::AdmissionOutcomes,
    IdKind::RoleProjection,
    IdKind::CostReservation,
];

/// Mirror of one Campaign identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireId {
    kind: u8,
    digest: [u8; DIGEST_LENGTH],
}

impl From<Id> for WireId {
    fn from(id: Id) -> Self {
        Self {
            kind: id.kind.code(),
            digest: id.digest,
        }
    }
}

impl TryFrom<WireId> for Id {
    type Error = WireError;

    fn try_from(wire: WireId) -> WireResult<Self> {
        let kind = ID_KINDS
            .into_iter()
            .find(|kind| kind.code() == wire.kind)
            .ok_or(WireError::UnknownIdKind(wire.kind))?;
        Ok(Self {
            kind,
            digest: wire.digest,
        })
    }
}

fn ids(values: &[Id]) -> Vec<WireId> {
    values.iter().copied().map(WireId::from).collect()
}

fn from_ids(values: Vec<WireId>) -> WireResult<Vec<Id>> {
    values.into_iter().map(Id::try_from).collect()
}

fn opt(value: Option<Id>) -> Option<WireId> {
    value.map(WireId::from)
}

fn from_opt(value: Option<WireId>) -> WireResult<Option<Id>> {
    value.map(Id::try_from).transpose()
}

fn id(value: WireId) -> WireResult<Id> {
    Id::try_from(value)
}

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireResource {
    dimension_id: WireId,
    amount: u64,
}

fn resources(values: &[ResourceAmount]) -> Vec<WireResource> {
    values
        .iter()
        .map(|value| WireResource {
            dimension_id: value.dimension_id.into(),
            amount: value.amount,
        })
        .collect()
}

fn from_resources(values: Vec<WireResource>) -> WireResult<Vec<ResourceAmount>> {
    values
        .into_iter()
        .map(|value| {
            Ok(ResourceAmount {
                dimension_id: id(value.dimension_id)?,
                amount: value.amount,
            })
        })
        .collect()
}

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireProjection {
    projection_id: WireId,
    graph_id: WireId,
    control_branch_id: WireId,
    control_branch_name_id: WireId,
    generation: u64,
    head_id: WireId,
    projection_schema_id: WireId,
    adapter_id: WireId,
    control_domain_id: WireId,
}

impl From<&ProjectionBinding> for WireProjection {
    fn from(value: &ProjectionBinding) -> Self {
        Self {
            projection_id: value.projection_id.into(),
            graph_id: value.graph_id.into(),
            control_branch_id: value.control_branch_id.into(),
            control_branch_name_id: value.control_branch_name_id.into(),
            generation: value.generation,
            head_id: value.head_id.into(),
            projection_schema_id: value.projection_schema_id.into(),
            adapter_id: value.adapter_id.into(),
            control_domain_id: value.control_domain_id.into(),
        }
    }
}

impl TryFrom<WireProjection> for ProjectionBinding {
    type Error = WireError;

    fn try_from(value: WireProjection) -> WireResult<Self> {
        Ok(Self {
            projection_id: id(value.projection_id)?,
            graph_id: id(value.graph_id)?,
            control_branch_id: id(value.control_branch_id)?,
            control_branch_name_id: id(value.control_branch_name_id)?,
            generation: value.generation,
            head_id: id(value.head_id)?,
            projection_schema_id: id(value.projection_schema_id)?,
            adapter_id: id(value.adapter_id)?,
            control_domain_id: id(value.control_domain_id)?,
        })
    }
}

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireBounds {
    bounds_id: WireId,
    values: [u64; 17],
}

impl From<&ExplorationBounds> for WireBounds {
    fn from(value: &ExplorationBounds) -> Self {
        Self {
            bounds_id: value.bounds_id.into(),
            values: [
                value.max_roots,
                value.max_candidates,
                value.max_candidate_batch,
                value.max_retained_candidates,
                value.max_selections,
                value.max_guidance_records,
                value.max_outstanding,
                value.max_depth,
                value.max_child_moments,
                value.max_derived_records,
                value.max_rank_dimensions,
                value.max_rank_magnitude,
                value.max_findings,
                value.max_control_events,
                value.max_resource_dimensions,
                value.max_operation_schemas,
                value.max_prune_batch,
            ],
        }
    }
}

impl TryFrom<WireBounds> for ExplorationBounds {
    type Error = WireError;

    fn try_from(value: WireBounds) -> WireResult<Self> {
        let [max_roots, max_candidates, max_candidate_batch, max_retained_candidates, max_selections, max_guidance_records, max_outstanding, max_depth, max_child_moments, max_derived_records, max_rank_dimensions, max_rank_magnitude, max_findings, max_control_events, max_resource_dimensions, max_operation_schemas, max_prune_batch] =
            value.values;
        Ok(Self {
            bounds_id: id(value.bounds_id)?,
            max_roots,
            max_candidates,
            max_candidate_batch,
            max_retained_candidates,
            max_selections,
            max_guidance_records,
            max_outstanding,
            max_depth,
            max_child_moments,
            max_derived_records,
            max_rank_dimensions,
            max_rank_magnitude,
            max_findings,
            max_control_events,
            max_resource_dimensions,
            max_operation_schemas,
            max_prune_batch,
        })
    }
}

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireRoot {
    moment_id: WireId,
    projection_id: WireId,
    adapter_id: WireId,
}

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireAdmission {
    campaign_id: WireId,
    schema_version: u32,
    projection: WireProjection,
    policy_id: WireId,
    guidance_profile_id: WireId,
    entropy_profile_id: WireId,
    budget_profile_id: WireId,
    adapter_id: WireId,
    control_domain_id: WireId,
    bounds: WireBounds,
    resource_budgets: Vec<WireResource>,
    roots: Vec<WireRoot>,
    operation_schema_ids: Vec<WireId>,
}

impl From<&CampaignAdmission> for WireAdmission {
    fn from(value: &CampaignAdmission) -> Self {
        Self {
            campaign_id: value.campaign_id.into(),
            schema_version: value.schema_version,
            projection: (&value.projection).into(),
            policy_id: value.policy_id.into(),
            guidance_profile_id: value.guidance_profile_id.into(),
            entropy_profile_id: value.entropy_profile_id.into(),
            budget_profile_id: value.budget_profile_id.into(),
            adapter_id: value.adapter_id.into(),
            control_domain_id: value.control_domain_id.into(),
            bounds: (&value.bounds).into(),
            resource_budgets: resources(&value.resource_budgets),
            roots: value
                .roots
                .iter()
                .map(|root| WireRoot {
                    moment_id: root.moment_id.into(),
                    projection_id: root.projection_id.into(),
                    adapter_id: root.adapter_id.into(),
                })
                .collect(),
            operation_schema_ids: ids(&value.operation_schema_ids),
        }
    }
}

impl TryFrom<WireAdmission> for CampaignAdmission {
    type Error = WireError;

    fn try_from(value: WireAdmission) -> WireResult<Self> {
        let roots = value
            .roots
            .into_iter()
            .map(|root| {
                Ok(RootMoment {
                    moment_id: id(root.moment_id)?,
                    projection_id: id(root.projection_id)?,
                    adapter_id: id(root.adapter_id)?,
                })
            })
            .collect::<WireResult<Vec<_>>>()?;
        Ok(Self {
            campaign_id: id(value.campaign_id)?,
            schema_version: value.schema_version,
            projection: value.projection.try_into()?,
            policy_id: id(value.policy_id)?,
            guidance_profile_id: id(value.guidance_profile_id)?,
            entropy_profile_id: id(value.entropy_profile_id)?,
            budget_profile_id: id(value.budget_profile_id)?,
            adapter_id: id(value.adapter_id)?,
            control_domain_id: id(value.control_domain_id)?,
            bounds: value.bounds.try_into()?,
            resource_budgets: from_resources(value.resource_budgets)?,
            roots,
            operation_schema_ids: from_ids(value.operation_schema_ids)?,
        })
    }
}

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireCandidate {
    candidate_id: WireId,
    campaign_id: WireId,
    parent_moment_id: WireId,
    operation_profile_id: WireId,
    operation_schema_id: WireId,
    source_adapter_id: WireId,
    depth: u64,
    cost_profile_id: WireId,
    declared_costs: Vec<WireResource>,
    operation_generation_profile_id: WireId,
}

impl From<&CandidateRecord> for WireCandidate {
    fn from(value: &CandidateRecord) -> Self {
        Self {
            candidate_id: value.candidate_id.into(),
            campaign_id: value.campaign_id.into(),
            parent_moment_id: value.parent_moment_id.into(),
            operation_profile_id: value.operation_profile_id.into(),
            operation_schema_id: value.operation_schema_id.into(),
            source_adapter_id: value.source_adapter_id.into(),
            depth: value.depth,
            cost_profile_id: value.cost_profile_id.into(),
            declared_costs: resources(&value.declared_costs),
            operation_generation_profile_id: value.operation_generation_profile_id.into(),
        }
    }
}

impl TryFrom<WireCandidate> for CandidateRecord {
    type Error = WireError;

    fn try_from(value: WireCandidate) -> WireResult<Self> {
        Ok(Self {
            candidate_id: id(value.candidate_id)?,
            campaign_id: id(value.campaign_id)?,
            parent_moment_id: id(value.parent_moment_id)?,
            operation_profile_id: id(value.operation_profile_id)?,
            operation_schema_id: id(value.operation_schema_id)?,
            source_adapter_id: id(value.source_adapter_id)?,
            depth: value.depth,
            cost_profile_id: id(value.cost_profile_id)?,
            declared_costs: from_resources(value.declared_costs)?,
            operation_generation_profile_id: id(value.operation_generation_profile_id)?,
        })
    }
}

const DUPLICATE_REJECT: u32 = 1;
const DUPLICATE_COALESCE: u32 = 2;

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireOutcome {
    candidate_id: WireId,
    coalesced_into: Option<WireId>,
}

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireCandidateBatch {
    event_id: WireId,
    campaign_id: WireId,
    state_before_id: WireId,
    candidates: Vec<WireCandidate>,
    duplicate_rule: u32,
    admission_policy_id: WireId,
    coalescing_policy_id: WireId,
    outcomes: Vec<WireOutcome>,
    admission_outcomes_id: WireId,
}

impl From<&CandidateBatchEvent> for WireCandidateBatch {
    fn from(value: &CandidateBatchEvent) -> Self {
        Self {
            event_id: value.event_id.into(),
            campaign_id: value.campaign_id.into(),
            state_before_id: value.state_before_id.into(),
            candidates: value.candidates.iter().map(WireCandidate::from).collect(),
            duplicate_rule: match value.admission_rule.duplicate_candidate {
                DuplicateCandidateRule::Reject => DUPLICATE_REJECT,
                DuplicateCandidateRule::Coalesce => DUPLICATE_COALESCE,
            },
            admission_policy_id: value.admission_policy_id.into(),
            coalescing_policy_id: value.coalescing_policy_id.into(),
            outcomes: value
                .outcomes
                .iter()
                .map(|outcome| WireOutcome {
                    candidate_id: outcome.candidate_id.into(),
                    coalesced_into: match outcome.outcome {
                        AdmissionDecision::Admitted => None,
                        AdmissionDecision::CoalescedDuplicate {
                            existing_candidate_id,
                        } => Some(existing_candidate_id.into()),
                    },
                })
                .collect(),
            admission_outcomes_id: value.admission_outcomes_id.into(),
        }
    }
}

impl TryFrom<WireCandidateBatch> for CandidateBatchEvent {
    type Error = WireError;

    fn try_from(value: WireCandidateBatch) -> WireResult<Self> {
        let duplicate_candidate = match value.duplicate_rule {
            DUPLICATE_REJECT => DuplicateCandidateRule::Reject,
            DUPLICATE_COALESCE => DuplicateCandidateRule::Coalesce,
            code => return Err(WireError::UnknownCode(code)),
        };
        let outcomes = value
            .outcomes
            .into_iter()
            .map(|outcome| {
                Ok(AdmissionOutcome {
                    candidate_id: id(outcome.candidate_id)?,
                    outcome: match outcome.coalesced_into {
                        None => AdmissionDecision::Admitted,
                        Some(existing) => AdmissionDecision::CoalescedDuplicate {
                            existing_candidate_id: id(existing)?,
                        },
                    },
                })
            })
            .collect::<WireResult<Vec<_>>>()?;
        Ok(Self {
            event_id: id(value.event_id)?,
            campaign_id: id(value.campaign_id)?,
            state_before_id: id(value.state_before_id)?,
            candidates: value
                .candidates
                .into_iter()
                .map(CandidateRecord::try_from)
                .collect::<WireResult<Vec<_>>>()?,
            admission_rule: AdmissionRule {
                duplicate_candidate,
            },
            admission_policy_id: id(value.admission_policy_id)?,
            coalescing_policy_id: id(value.coalescing_policy_id)?,
            outcomes,
            admission_outcomes_id: id(value.admission_outcomes_id)?,
        })
    }
}

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireGuidance {
    guidance_id: WireId,
    campaign_id: WireId,
    candidate_id: WireId,
    generation: u64,
    ranks: Vec<i64>,
    source_facts_id: WireId,
    ordering_profile_id: WireId,
    adapter_id: WireId,
}

impl From<&GuidanceRecord> for WireGuidance {
    fn from(value: &GuidanceRecord) -> Self {
        Self {
            guidance_id: value.guidance_id.into(),
            campaign_id: value.campaign_id.into(),
            candidate_id: value.candidate_id.into(),
            generation: value.generation,
            ranks: value.ranks.clone(),
            source_facts_id: value.source_facts_id.into(),
            ordering_profile_id: value.ordering_profile_id.into(),
            adapter_id: value.adapter_id.into(),
        }
    }
}

impl TryFrom<WireGuidance> for GuidanceRecord {
    type Error = WireError;

    fn try_from(value: WireGuidance) -> WireResult<Self> {
        Ok(Self {
            guidance_id: id(value.guidance_id)?,
            campaign_id: id(value.campaign_id)?,
            candidate_id: id(value.candidate_id)?,
            generation: value.generation,
            ranks: value.ranks,
            source_facts_id: id(value.source_facts_id)?,
            ordering_profile_id: id(value.ordering_profile_id)?,
            adapter_id: id(value.adapter_id)?,
        })
    }
}

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum WireDynamics {
    Discovery,
    StrictImprovement {
        previous_guidance_id: WireId,
    },
    CoalescedDuplicate {
        existing_guidance_id: WireId,
    },
    Revision {
        invalidated_guidance_id: WireId,
        invalidation_id: WireId,
        reason_id: WireId,
    },
}

impl From<&GuidanceDynamics> for WireDynamics {
    fn from(value: &GuidanceDynamics) -> Self {
        match value {
            GuidanceDynamics::Discovery => Self::Discovery,
            GuidanceDynamics::StrictImprovement {
                previous_guidance_id,
            } => Self::StrictImprovement {
                previous_guidance_id: (*previous_guidance_id).into(),
            },
            GuidanceDynamics::CoalescedDuplicate {
                existing_guidance_id,
            } => Self::CoalescedDuplicate {
                existing_guidance_id: (*existing_guidance_id).into(),
            },
            GuidanceDynamics::Revision {
                invalidated_guidance_id,
                invalidation_id,
                reason_id,
            } => Self::Revision {
                invalidated_guidance_id: (*invalidated_guidance_id).into(),
                invalidation_id: (*invalidation_id).into(),
                reason_id: (*reason_id).into(),
            },
        }
    }
}

impl TryFrom<WireDynamics> for GuidanceDynamics {
    type Error = WireError;

    fn try_from(value: WireDynamics) -> WireResult<Self> {
        Ok(match value {
            WireDynamics::Discovery => Self::Discovery,
            WireDynamics::StrictImprovement {
                previous_guidance_id,
            } => Self::StrictImprovement {
                previous_guidance_id: id(previous_guidance_id)?,
            },
            WireDynamics::CoalescedDuplicate {
                existing_guidance_id,
            } => Self::CoalescedDuplicate {
                existing_guidance_id: id(existing_guidance_id)?,
            },
            WireDynamics::Revision {
                invalidated_guidance_id,
                invalidation_id,
                reason_id,
            } => Self::Revision {
                invalidated_guidance_id: id(invalidated_guidance_id)?,
                invalidation_id: id(invalidation_id)?,
                reason_id: id(reason_id)?,
            },
        })
    }
}

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireGuidanceEvent {
    event_id: WireId,
    campaign_id: WireId,
    state_before_id: WireId,
    guidance: WireGuidance,
    dynamics: WireDynamics,
    admission_outcomes_id: WireId,
}

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum WireMode {
    Ranked,
    Exploratory {
        threshold_parts_per_million: u64,
        entropy_source_id: WireId,
        exploration_order: Vec<u32>,
    },
}

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum WireDecay {
    None,
    SubtractFromFirstRank { penalty_per_selection: u64 },
}

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum WireImplementation {
    ProductionCompiled { policy_id: WireId },
    ReferenceEvidence { reference_id: WireId },
}

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireProfile {
    profile_id: WireId,
    scheduling_policy_id: WireId,
    admission_policy_id: WireId,
    coalescing_policy_id: WireId,
    mode: WireMode,
    decay: WireDecay,
    finding_limit: Option<u64>,
    implementation: WireImplementation,
}

impl From<&SelectionProfile> for WireProfile {
    fn from(value: &SelectionProfile) -> Self {
        Self {
            profile_id: value.profile_id.into(),
            scheduling_policy_id: value.scheduling_policy_id.into(),
            admission_policy_id: value.admission_policy_id.into(),
            coalescing_policy_id: value.coalescing_policy_id.into(),
            mode: match &value.mode {
                SelectionMode::Ranked => WireMode::Ranked,
                SelectionMode::Exploratory {
                    threshold_parts_per_million,
                    entropy_source_id,
                    exploration_order,
                } => WireMode::Exploratory {
                    threshold_parts_per_million: *threshold_parts_per_million,
                    entropy_source_id: (*entropy_source_id).into(),
                    exploration_order: exploration_order.clone(),
                },
            },
            decay: match &value.decay {
                SelectionCountDecay::None => WireDecay::None,
                SelectionCountDecay::SubtractFromFirstRank {
                    penalty_per_selection,
                } => WireDecay::SubtractFromFirstRank {
                    penalty_per_selection: *penalty_per_selection,
                },
            },
            finding_limit: value.stopping.finding_limit,
            implementation: match &value.implementation {
                PolicyImplementation::ProductionCompiled { policy_id } => {
                    WireImplementation::ProductionCompiled {
                        policy_id: (*policy_id).into(),
                    }
                }
                PolicyImplementation::ReferenceEvidence { reference_id } => {
                    WireImplementation::ReferenceEvidence {
                        reference_id: (*reference_id).into(),
                    }
                }
            },
        }
    }
}

impl TryFrom<WireProfile> for SelectionProfile {
    type Error = WireError;

    fn try_from(value: WireProfile) -> WireResult<Self> {
        Ok(Self {
            profile_id: id(value.profile_id)?,
            scheduling_policy_id: id(value.scheduling_policy_id)?,
            admission_policy_id: id(value.admission_policy_id)?,
            coalescing_policy_id: id(value.coalescing_policy_id)?,
            mode: match value.mode {
                WireMode::Ranked => SelectionMode::Ranked,
                WireMode::Exploratory {
                    threshold_parts_per_million,
                    entropy_source_id,
                    exploration_order,
                } => SelectionMode::Exploratory {
                    threshold_parts_per_million,
                    entropy_source_id: id(entropy_source_id)?,
                    exploration_order,
                },
            },
            decay: match value.decay {
                WireDecay::None => SelectionCountDecay::None,
                WireDecay::SubtractFromFirstRank {
                    penalty_per_selection,
                } => SelectionCountDecay::SubtractFromFirstRank {
                    penalty_per_selection,
                },
            },
            stopping: StoppingRule {
                finding_limit: value.finding_limit,
            },
            implementation: match value.implementation {
                WireImplementation::ProductionCompiled { policy_id } => {
                    PolicyImplementation::ProductionCompiled {
                        policy_id: id(policy_id)?,
                    }
                }
                WireImplementation::ReferenceEvidence { reference_id } => {
                    PolicyImplementation::ReferenceEvidence {
                        reference_id: id(reference_id)?,
                    }
                }
            },
        })
    }
}

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireTicket {
    entropy_id: WireId,
    campaign_id: WireId,
    state_id: WireId,
    profile_id: WireId,
    source_id: WireId,
    counter: u64,
    bytes: [u8; DIGEST_LENGTH],
}

impl From<&EntropyTicket> for WireTicket {
    fn from(value: &EntropyTicket) -> Self {
        Self {
            entropy_id: value.entropy_id.into(),
            campaign_id: value.campaign_id.into(),
            state_id: value.state_id.into(),
            profile_id: value.profile_id.into(),
            source_id: value.source_id.into(),
            counter: value.counter,
            bytes: value.bytes,
        }
    }
}

impl TryFrom<WireTicket> for EntropyTicket {
    type Error = WireError;

    fn try_from(value: WireTicket) -> WireResult<Self> {
        Ok(Self {
            entropy_id: id(value.entropy_id)?,
            campaign_id: id(value.campaign_id)?,
            state_id: id(value.state_id)?,
            profile_id: id(value.profile_id)?,
            source_id: id(value.source_id)?,
            counter: value.counter,
            bytes: value.bytes,
        })
    }
}

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WirePlan {
    event_id: WireId,
    campaign_id: WireId,
    control_branch_id: WireId,
    expected_head_id: WireId,
    expected_generation: u64,
    state_before_id: WireId,
    selection_id: WireId,
}

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireDecision {
    selection_id: WireId,
    ordinal: u64,
    campaign_id: WireId,
    state_before_id: WireId,
    candidate_id: WireId,
    parent_moment_id: WireId,
    operation_id: WireId,
    guidance_id: WireId,
    profile_id: WireId,
    entropy_id: Option<WireId>,
    reserved_costs: Vec<WireResource>,
    publication_plan: WirePlan,
}

impl From<&SelectionDecision> for WireDecision {
    fn from(value: &SelectionDecision) -> Self {
        let plan = &value.publication_plan;
        Self {
            selection_id: value.selection_id.into(),
            ordinal: value.ordinal,
            campaign_id: value.campaign_id.into(),
            state_before_id: value.state_before_id.into(),
            candidate_id: value.candidate_id.into(),
            parent_moment_id: value.parent_moment_id.into(),
            operation_id: value.operation_id.into(),
            guidance_id: value.guidance_id.into(),
            profile_id: value.profile_id.into(),
            entropy_id: opt(value.entropy_id),
            reserved_costs: resources(&value.reserved_costs),
            publication_plan: WirePlan {
                event_id: plan.event_id.into(),
                campaign_id: plan.campaign_id.into(),
                control_branch_id: plan.control_branch_id.into(),
                expected_head_id: plan.expected_head_id.into(),
                expected_generation: plan.expected_generation,
                state_before_id: plan.state_before_id.into(),
                selection_id: plan.selection_id.into(),
            },
        }
    }
}

impl TryFrom<WireDecision> for SelectionDecision {
    type Error = WireError;

    fn try_from(value: WireDecision) -> WireResult<Self> {
        let plan = value.publication_plan;
        Ok(Self {
            selection_id: id(value.selection_id)?,
            ordinal: value.ordinal,
            campaign_id: id(value.campaign_id)?,
            state_before_id: id(value.state_before_id)?,
            candidate_id: id(value.candidate_id)?,
            parent_moment_id: id(value.parent_moment_id)?,
            operation_id: id(value.operation_id)?,
            guidance_id: id(value.guidance_id)?,
            profile_id: id(value.profile_id)?,
            entropy_id: from_opt(value.entropy_id)?,
            reserved_costs: from_resources(value.reserved_costs)?,
            publication_plan: PublicationPlan {
                event_id: id(plan.event_id)?,
                campaign_id: id(plan.campaign_id)?,
                control_branch_id: id(plan.control_branch_id)?,
                expected_head_id: id(plan.expected_head_id)?,
                expected_generation: plan.expected_generation,
                state_before_id: id(plan.state_before_id)?,
                selection_id: id(plan.selection_id)?,
            },
        })
    }
}

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireSelectionEvent {
    event_id: WireId,
    decision: WireDecision,
    profile: WireProfile,
    entropy: Option<WireTicket>,
}

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireChild {
    reservation_id: WireId,
    child_moment_id: WireId,
    discovery_id: WireId,
    event_payload_id: WireId,
}

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireFinding {
    finding_id: WireId,
    priority_facts_id: WireId,
}

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireObservation {
    result_id: WireId,
    campaign_id: WireId,
    selection_id: WireId,
    parent_moment_id: WireId,
    operation_id: WireId,
    adapter_id: WireId,
    payload_id: WireId,
    children: Vec<WireChild>,
    findings: Vec<WireFinding>,
    derived_records: Vec<WireId>,
}

impl From<&ResultObservation> for WireObservation {
    fn from(value: &ResultObservation) -> Self {
        Self {
            result_id: value.result_id.into(),
            campaign_id: value.campaign_id.into(),
            selection_id: value.selection_id.into(),
            parent_moment_id: value.parent_moment_id.into(),
            operation_id: value.operation_id.into(),
            adapter_id: value.adapter_id.into(),
            payload_id: value.payload_id.into(),
            children: value
                .children
                .iter()
                .map(|child| WireChild {
                    reservation_id: child.reservation_id.into(),
                    child_moment_id: child.child_moment_id.into(),
                    discovery_id: child.discovery_id.into(),
                    event_payload_id: child.event_payload_id.into(),
                })
                .collect(),
            findings: value
                .findings
                .iter()
                .map(|finding| WireFinding {
                    finding_id: finding.finding_id.into(),
                    priority_facts_id: finding.priority_facts_id.into(),
                })
                .collect(),
            derived_records: ids(&value.derived_records),
        }
    }
}

impl TryFrom<WireObservation> for ResultObservation {
    type Error = WireError;

    fn try_from(value: WireObservation) -> WireResult<Self> {
        Ok(Self {
            result_id: id(value.result_id)?,
            campaign_id: id(value.campaign_id)?,
            selection_id: id(value.selection_id)?,
            parent_moment_id: id(value.parent_moment_id)?,
            operation_id: id(value.operation_id)?,
            adapter_id: id(value.adapter_id)?,
            payload_id: id(value.payload_id)?,
            children: value
                .children
                .into_iter()
                .map(|child| {
                    Ok(ChildObservation {
                        reservation_id: id(child.reservation_id)?,
                        child_moment_id: id(child.child_moment_id)?,
                        discovery_id: id(child.discovery_id)?,
                        event_payload_id: id(child.event_payload_id)?,
                    })
                })
                .collect::<WireResult<Vec<_>>>()?,
            findings: value
                .findings
                .into_iter()
                .map(|finding| {
                    Ok(FindingRef {
                        finding_id: id(finding.finding_id)?,
                        priority_facts_id: id(finding.priority_facts_id)?,
                    })
                })
                .collect::<WireResult<Vec<_>>>()?,
            derived_records: from_ids(value.derived_records)?,
        })
    }
}

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireResultEvent {
    event_id: WireId,
    campaign_id: WireId,
    state_before_id: WireId,
    observation: WireObservation,
}

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireClosureEvent {
    event_id: WireId,
    campaign_id: WireId,
    state_before_id: WireId,
    closure_id: WireId,
    closure_campaign_id: WireId,
    selection_id: WireId,
    reason_id: WireId,
    uncertainty_acknowledged: bool,
}

const ORIGIN_CONSUMER: u32 = 1;
const ORIGIN_POLICY: u32 = 2;

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireRetirementEvent {
    event_id: WireId,
    campaign_id: WireId,
    state_before_id: WireId,
    candidate_id: WireId,
    reason_id: WireId,
    policy_id: WireId,
    adapter_id: WireId,
    origin: u32,
    invalidation_id: WireId,
}

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WirePruneEvent {
    event_id: WireId,
    campaign_id: WireId,
    state_before_id: WireId,
    pruned_candidates: Vec<WireId>,
    prune_plan_id: WireId,
    reason_id: WireId,
    retention_profile_id: Option<WireId>,
    rank_order: Vec<u32>,
    invalidation_id: WireId,
}

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum WireStopReason {
    FrontierExhausted,
    BudgetExhausted { dimensions: Vec<WireId> },
    ConsumerRequested { reason_id: WireId },
    PolicyRequested { reason_id: WireId },
    SelectionLimitReached,
    ControlEventLimitReached,
}

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireStopEvent {
    event_id: WireId,
    campaign_id: WireId,
    state_before_id: WireId,
    stop_id: WireId,
    stop_campaign_id: WireId,
    stop_state_before_id: WireId,
    reason: WireStopReason,
    incomplete_search_acknowledged: bool,
}

/// Serde mirror of every Campaign control event.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum WireControlEvent {
    /// Mirror of `CampaignAdmitted`.
    CampaignAdmitted {
        /// Control event identity.
        event_id: WireId,
        /// Admission payload.
        admission: Box<WireAdmission>,
    },
    /// Mirror of `CandidatesAdmitted`.
    CandidatesAdmitted(Box<WireCandidateBatch>),
    /// Mirror of `GuidanceAdmitted`.
    GuidanceAdmitted(Box<WireGuidanceEvent>),
    /// Mirror of `SelectionPublished`.
    SelectionPublished(Box<WireSelectionEvent>),
    /// Mirror of `ResultObserved`.
    ResultObserved(Box<WireResultEvent>),
    /// Mirror of `UnobservedClosed`.
    UnobservedClosed(Box<WireClosureEvent>),
    /// Mirror of `CandidateRetired`.
    CandidateRetired(Box<WireRetirementEvent>),
    /// Mirror of `CandidatesPruned`.
    CandidatesPruned(Box<WirePruneEvent>),
    /// Mirror of `StopAccepted`.
    StopAccepted(Box<WireStopEvent>),
}

impl From<&CampaignControlEvent> for WireControlEvent {
    fn from(event: &CampaignControlEvent) -> Self {
        match event {
            CampaignControlEvent::CampaignAdmitted {
                event_id,
                admission,
            } => Self::CampaignAdmitted {
                event_id: (*event_id).into(),
                admission: Box::new(admission.into()),
            },
            CampaignControlEvent::CandidatesAdmitted(batch) => {
                Self::CandidatesAdmitted(Box::new(batch.into()))
            }
            CampaignControlEvent::GuidanceAdmitted(value) => {
                Self::GuidanceAdmitted(Box::new(WireGuidanceEvent {
                    event_id: value.event_id.into(),
                    campaign_id: value.campaign_id.into(),
                    state_before_id: value.state_before_id.into(),
                    guidance: (&value.guidance).into(),
                    dynamics: (&value.dynamics).into(),
                    admission_outcomes_id: value.admission_outcomes_id.into(),
                }))
            }
            CampaignControlEvent::SelectionPublished(value) => {
                Self::SelectionPublished(Box::new(WireSelectionEvent {
                    event_id: value.event_id.into(),
                    decision: (&value.decision).into(),
                    profile: value.profile.as_ref().into(),
                    entropy: value.entropy.as_ref().map(WireTicket::from),
                }))
            }
            CampaignControlEvent::ResultObserved(value) => {
                Self::ResultObserved(Box::new(WireResultEvent {
                    event_id: value.event_id.into(),
                    campaign_id: value.campaign_id.into(),
                    state_before_id: value.state_before_id.into(),
                    observation: (&value.observation).into(),
                }))
            }
            CampaignControlEvent::UnobservedClosed(value) => {
                Self::UnobservedClosed(Box::new(WireClosureEvent {
                    event_id: value.event_id.into(),
                    campaign_id: value.campaign_id.into(),
                    state_before_id: value.state_before_id.into(),
                    closure_id: value.closure.closure_id.into(),
                    closure_campaign_id: value.closure.campaign_id.into(),
                    selection_id: value.closure.selection_id.into(),
                    reason_id: value.closure.reason_id.into(),
                    uncertainty_acknowledged: value.closure.uncertainty_acknowledged,
                }))
            }
            CampaignControlEvent::CandidateRetired(value) => {
                Self::CandidateRetired(Box::new(WireRetirementEvent {
                    event_id: value.event_id.into(),
                    campaign_id: value.campaign_id.into(),
                    state_before_id: value.state_before_id.into(),
                    candidate_id: value.candidate_id.into(),
                    reason_id: value.reason_id.into(),
                    policy_id: value.policy_id.into(),
                    adapter_id: value.adapter_id.into(),
                    origin: value.origin.code(),
                    invalidation_id: value.invalidation_id.into(),
                }))
            }
            CampaignControlEvent::CandidatesPruned(value) => {
                Self::CandidatesPruned(Box::new(WirePruneEvent {
                    event_id: value.event_id.into(),
                    campaign_id: value.campaign_id.into(),
                    state_before_id: value.state_before_id.into(),
                    pruned_candidates: ids(&value.pruned_candidates),
                    prune_plan_id: value.prune_plan_id.into(),
                    reason_id: value.reason_id.into(),
                    retention_profile_id: opt(value.retention_profile_id),
                    rank_order: value.rank_order.clone(),
                    invalidation_id: value.invalidation_id.into(),
                }))
            }
            CampaignControlEvent::StopAccepted(value) => {
                Self::StopAccepted(Box::new(WireStopEvent {
                    event_id: value.event_id.into(),
                    campaign_id: value.campaign_id.into(),
                    state_before_id: value.state_before_id.into(),
                    stop_id: value.stop.stop_id.into(),
                    stop_campaign_id: value.stop.campaign_id.into(),
                    stop_state_before_id: value.stop.state_before_id.into(),
                    reason: stop_reason_to_wire(&value.stop.reason),
                    incomplete_search_acknowledged: value.stop.incomplete_search_acknowledged,
                }))
            }
        }
    }
}

fn stop_reason_to_wire(reason: &StopReason) -> WireStopReason {
    match reason {
        StopReason::FrontierExhausted => WireStopReason::FrontierExhausted,
        StopReason::BudgetExhausted { dimensions } => WireStopReason::BudgetExhausted {
            dimensions: ids(dimensions),
        },
        StopReason::ConsumerRequested { reason_id } => WireStopReason::ConsumerRequested {
            reason_id: (*reason_id).into(),
        },
        StopReason::PolicyRequested { reason_id } => WireStopReason::PolicyRequested {
            reason_id: (*reason_id).into(),
        },
        StopReason::SelectionLimitReached => WireStopReason::SelectionLimitReached,
        StopReason::ControlEventLimitReached => WireStopReason::ControlEventLimitReached,
    }
}

fn stop_reason_from_wire(reason: WireStopReason) -> WireResult<StopReason> {
    Ok(match reason {
        WireStopReason::FrontierExhausted => StopReason::FrontierExhausted,
        WireStopReason::BudgetExhausted { dimensions } => StopReason::BudgetExhausted {
            dimensions: from_ids(dimensions)?,
        },
        WireStopReason::ConsumerRequested { reason_id } => StopReason::ConsumerRequested {
            reason_id: id(reason_id)?,
        },
        WireStopReason::PolicyRequested { reason_id } => StopReason::PolicyRequested {
            reason_id: id(reason_id)?,
        },
        WireStopReason::SelectionLimitReached => StopReason::SelectionLimitReached,
        WireStopReason::ControlEventLimitReached => StopReason::ControlEventLimitReached,
    })
}

impl TryFrom<WireControlEvent> for CampaignControlEvent {
    type Error = WireError;

    fn try_from(wire: WireControlEvent) -> WireResult<Self> {
        Ok(match wire {
            WireControlEvent::CampaignAdmitted {
                event_id,
                admission,
            } => Self::CampaignAdmitted {
                event_id: id(event_id)?,
                admission: (*admission).try_into()?,
            },
            WireControlEvent::CandidatesAdmitted(batch) => {
                Self::CandidatesAdmitted((*batch).try_into()?)
            }
            WireControlEvent::GuidanceAdmitted(value) => Self::GuidanceAdmitted(GuidanceEvent {
                event_id: id(value.event_id)?,
                campaign_id: id(value.campaign_id)?,
                state_before_id: id(value.state_before_id)?,
                guidance: value.guidance.try_into()?,
                dynamics: value.dynamics.try_into()?,
                admission_outcomes_id: id(value.admission_outcomes_id)?,
            }),
            WireControlEvent::SelectionPublished(value) => {
                Self::SelectionPublished(SelectionEvent {
                    event_id: id(value.event_id)?,
                    decision: value.decision.try_into()?,
                    profile: Box::new(value.profile.try_into()?),
                    entropy: value.entropy.map(EntropyTicket::try_from).transpose()?,
                })
            }
            WireControlEvent::ResultObserved(value) => Self::ResultObserved(ResultEvent {
                event_id: id(value.event_id)?,
                campaign_id: id(value.campaign_id)?,
                state_before_id: id(value.state_before_id)?,
                observation: value.observation.try_into()?,
            }),
            WireControlEvent::UnobservedClosed(value) => Self::UnobservedClosed(ClosureEvent {
                event_id: id(value.event_id)?,
                campaign_id: id(value.campaign_id)?,
                state_before_id: id(value.state_before_id)?,
                closure: UnobservedClosure {
                    closure_id: id(value.closure_id)?,
                    campaign_id: id(value.closure_campaign_id)?,
                    selection_id: id(value.selection_id)?,
                    reason_id: id(value.reason_id)?,
                    uncertainty_acknowledged: value.uncertainty_acknowledged,
                },
            }),
            WireControlEvent::CandidateRetired(value) => Self::CandidateRetired(RetirementEvent {
                event_id: id(value.event_id)?,
                campaign_id: id(value.campaign_id)?,
                state_before_id: id(value.state_before_id)?,
                candidate_id: id(value.candidate_id)?,
                reason_id: id(value.reason_id)?,
                policy_id: id(value.policy_id)?,
                adapter_id: id(value.adapter_id)?,
                origin: match value.origin {
                    ORIGIN_CONSUMER => RetirementOrigin::Consumer,
                    ORIGIN_POLICY => RetirementOrigin::Policy,
                    code => return Err(WireError::UnknownCode(code)),
                },
                invalidation_id: id(value.invalidation_id)?,
            }),
            WireControlEvent::CandidatesPruned(value) => Self::CandidatesPruned(PruneEvent {
                event_id: id(value.event_id)?,
                campaign_id: id(value.campaign_id)?,
                state_before_id: id(value.state_before_id)?,
                pruned_candidates: from_ids(value.pruned_candidates)?,
                prune_plan_id: id(value.prune_plan_id)?,
                reason_id: id(value.reason_id)?,
                retention_profile_id: from_opt(value.retention_profile_id)?,
                rank_order: value.rank_order,
                invalidation_id: id(value.invalidation_id)?,
            }),
            WireControlEvent::StopAccepted(value) => Self::StopAccepted(StopEvent {
                event_id: id(value.event_id)?,
                campaign_id: id(value.campaign_id)?,
                state_before_id: id(value.state_before_id)?,
                stop: StopDecision {
                    stop_id: id(value.stop_id)?,
                    campaign_id: id(value.stop_campaign_id)?,
                    state_before_id: id(value.stop_state_before_id)?,
                    reason: stop_reason_from_wire(value.reason)?,
                    incomplete_search_acknowledged: value.incomplete_search_acknowledged,
                },
            }),
        })
    }
}

/// Journal record of one control event with its Choregraph control-payload identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireControlRecord {
    /// `EventPayload` identity Choregraph stores for the event.
    pub payload_id: WireId,
    /// Mirrored control event.
    pub event: WireControlEvent,
}

/// Re-derives every recorded Campaign identity of one decoded control event.
///
/// # Errors
/// Returns `IdentityMismatch` when the event, payload, or admission identity differs.
pub fn authenticate_control_event(event: &CampaignControlEvent, payload_id: Id) -> WireResult<()> {
    if event.expected_event_id()? != event.event_id() {
        return Err(WireError::IdentityMismatch);
    }
    if derive_control_payload_id(event)? != payload_id {
        return Err(WireError::IdentityMismatch);
    }
    if let CampaignControlEvent::CampaignAdmitted { admission, .. } = event {
        admission
            .validate()
            .map_err(|_| WireError::IdentityMismatch)?;
    }
    Ok(())
}

fn to_cbor<T: Serialize>(value: &T) -> WireResult<Vec<u8>> {
    let mut encoded = Vec::new();
    ciborium::ser::into_writer(value, &mut encoded).map_err(|_| WireError::Encode)?;
    Ok(encoded)
}

fn from_cbor<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> WireResult<T> {
    let mut reader = bytes;
    let value = ciborium::de::from_reader(&mut reader).map_err(|_| WireError::Decode)?;
    if !reader.is_empty() {
        return Err(WireError::Decode);
    }
    Ok(value)
}

/// Encodes one control event and its control-payload identity as CBOR.
///
/// # Errors
/// Returns a Campaign error when the payload identity cannot be derived.
pub fn encode_control_event(event: &CampaignControlEvent) -> WireResult<Vec<u8>> {
    let record = WireControlRecord {
        payload_id: derive_control_payload_id(event)?.into(),
        event: event.into(),
    };
    to_cbor(&record)
}

/// Decodes and authenticates one CBOR control-event record.
///
/// # Errors
/// Returns `Decode`, an unknown-code error, or `IdentityMismatch`.
pub fn decode_control_event(bytes: &[u8]) -> WireResult<CampaignControlEvent> {
    let record: WireControlRecord = from_cbor(bytes)?;
    let payload_id = id(record.payload_id)?;
    let event = CampaignControlEvent::try_from(record.event)?;
    authenticate_control_event(&event, payload_id)?;
    Ok(event)
}

const CLASS_APPLICATION: u8 = 0;
const CLASS_MACHINE_V1: u8 = 1;
const CLASS_MACHINE_V2: u8 = 2;
const CLASS_MERGE: u8 = 3;

/// Journal mirror value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireLabelled {
    label: String,
    digest: [u8; 32],
}

/// Serde mirror of one Choregraph moment event and its recorded identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireMoment {
    event: [u8; 32],
    version: u16,
    class: u8,
    payload: [u8; 32],
    parents: Vec<WireLabelled>,
    schema: [u8; 32],
    projection: Option<[u8; 32]>,
    metadata: Vec<WireLabelled>,
}

/// Encodes one moment draft with its admitted event identity.
///
/// # Errors
/// Returns `IdentityMismatch` when `event` is not the draft's identity.
pub fn encode_moment(event: EventId, draft: &EventDraft) -> WireResult<Vec<u8>> {
    if HistoryGraph::expected_identity(draft) != event {
        return Err(WireError::IdentityMismatch);
    }
    let class = match draft.class {
        EventClass::Application => CLASS_APPLICATION,
        EventClass::MachineV1 => CLASS_MACHINE_V1,
        EventClass::MachineV2 => CLASS_MACHINE_V2,
        EventClass::Merge => CLASS_MERGE,
    };
    to_cbor(&WireMoment {
        event: event.digest().bytes(),
        version: draft.version,
        class,
        payload: draft.payload_identity.bytes(),
        parents: draft
            .parents
            .iter()
            .map(|parent| WireLabelled {
                label: parent.role.clone(),
                digest: parent.event.digest().bytes(),
            })
            .collect(),
        schema: draft.schema_identity.bytes(),
        projection: draft.projection_identity.map(Digest::bytes),
        metadata: draft
            .metadata
            .iter()
            .map(|entry| WireLabelled {
                label: entry.key.clone(),
                digest: entry.value.bytes(),
            })
            .collect(),
    })
}

/// Decodes one moment record and re-derives its Choregraph event identity.
///
/// # Errors
/// Returns `Decode`, an unknown class code, or `IdentityMismatch`.
pub fn decode_moment(bytes: &[u8]) -> WireResult<(EventId, EventDraft)> {
    let wire: WireMoment = from_cbor(bytes)?;
    let class = match wire.class {
        CLASS_APPLICATION => EventClass::Application,
        CLASS_MACHINE_V1 => EventClass::MachineV1,
        CLASS_MACHINE_V2 => EventClass::MachineV2,
        CLASS_MERGE => EventClass::Merge,
        code => return Err(WireError::UnknownCode(u32::from(code))),
    };
    let draft = EventDraft {
        version: wire.version,
        class,
        payload_identity: Digest::from_bytes(wire.payload),
        parents: wire
            .parents
            .into_iter()
            .map(|parent| ParentLink {
                role: parent.label,
                event: EventId::new(Digest::from_bytes(parent.digest)),
            })
            .collect(),
        schema_identity: Digest::from_bytes(wire.schema),
        projection_identity: wire.projection.map(Digest::from_bytes),
        metadata: wire
            .metadata
            .into_iter()
            .map(|entry| MetadataIdentity {
                key: entry.label,
                value: Digest::from_bytes(entry.digest),
            })
            .collect(),
    };
    let event = EventId::new(Digest::from_bytes(wire.event));
    if HistoryGraph::expected_identity(&draft) != event {
        return Err(WireError::IdentityMismatch);
    }
    Ok((event, draft))
}
