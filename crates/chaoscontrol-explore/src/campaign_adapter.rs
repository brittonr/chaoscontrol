//! Pure ChaosControl adapter to Campaign policy and Choregraph history values.
//!
//! Every function here maps explicit product facts to Campaign or Choregraph
//! values, or maps Campaign decisions back to product meaning. Nothing here
//! touches the filesystem, clocks, workers, snapshots, or KVM; the Explorer
//! shell owns those effects and the durable journal. Campaign and Choregraph
//! receipts stay structural and never satisfy VM, fault, assertion, finding,
//! replay, minimization, or release evidence.

pub mod candidates;
pub mod eligibility;
pub mod entropy;
pub mod fence;
pub mod identity;
pub mod moments;
pub mod profile;
pub mod ranks;
pub mod results;
pub mod stops;
pub mod wire;

pub use candidates::{
    candidate_record, generation_profile_id, marker_digest, operation_profile_id,
    validate_candidate, FrontierEntryFacts,
};
pub use eligibility::{
    ineligible_retirement, unobserved_closure, Eligibility, IneligibleReason, SnapshotContext,
    SnapshotEntry, SnapshotTable,
};
pub use entropy::{
    entropy_ticket, legacy_explores, selection_mode, ticket_counter, ticket_explores, EntropyDraws,
    LegacyDraws, EXPLORATION_ORDER, EXPLORATION_THRESHOLD_PARTS_PER_MILLION,
    LEGACY_EXPLORATION_EPSILON,
};
pub use fence::{
    advance_projection, execution_authority, load_projection, receipt_claims, validate_cache,
};
pub use identity::{
    entropy_rule_id, entropy_source_id, AdapterIdentity, AdapterReason, RAND_CHACHA_VERSION,
    RAND_VERSION,
};
pub use moments::{
    admit_child_moment, admit_root_moment, child_moment_draft, child_moment_id, child_payload_id,
    moment_schema, projection_binding, readmit_moment, root_control_branch, root_moment_draft,
    root_moment_id, root_payload_id, AdmittedMoment, BranchSlot, ChildMomentFacts, RootMomentFacts,
    CONTROL_BRANCH_NAME,
};
pub use profile::{
    admit_config, campaign_admission, retention_plan_request, selection_request, BootstrapFacts,
    CampaignProfile, DegenerateConfig, SelectionLimit, RETENTION_RANK_ORDER,
};
pub use ranks::{
    decay_revision, frontier_ranks, guidance_record, insertion_rank, rank_profile_id, score_rank,
    GuidanceFacts, RankError, MAX_RANK_MAGNITUDE, RANK_DIMENSIONS,
};
pub use results::{child_observation, finding_ref, result_observation, ResultFacts, RetainedChild};
pub use stops::{
    is_capacity_refusal, product_stop, round_decision, round_fits_capacity, stop_reason,
    HostCapacity, ProductStop, RoundDecision, RoundDemand,
};
pub use wire::{
    authenticate_control_event, decode_control_event, decode_moment, encode_control_event,
    encode_moment, WireError,
};

/// Typed failure of one pure adapter mapping. No mapping mutates product state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdapterError {
    /// Score-to-rank conversion rejected the product facts.
    Rank(RankError),
    /// Campaign rejected a derived value.
    Campaign(campaign_core::CampaignError),
    /// Campaign's Choregraph adapter rejected a projection or publication.
    Choregraph(campaign_choregraph::AdapterError),
    /// Choregraph history rejected a moment admission.
    History(campaign_choregraph::HistoryError),
    /// A value names another adapter, candidate, or moment than the one mapped.
    CrossedIdentity,
    /// A value names another campaign than the admitted one.
    CrossedCampaign,
    /// A decay revision does not advance the selection count by exactly one.
    StaleGuidanceGeneration,
    /// A recorded `gen::<f64>` draw is not an exact multiple of `2^-53` in `[0, 1)`.
    NonCanonicalUnitDraw,
    /// Product and Campaign explore predicates disagree for one draw.
    ExploreDisagreement,
    /// The explore branch ran without a recorded `gen_range` draw.
    MissingIndexDraw,
    /// The exploit branch carried a `gen_range` draw.
    UnexpectedIndexDraw,
    /// The recorded `gen_range` draw is outside the frontier.
    IndexDrawOutOfRange,
    /// An exploratory selection request carries no entropy ticket.
    MissingEntropyTicket,
    /// An event or parent is not a ChaosControl exploration moment.
    NotMoment,
    /// A candidate names a parent moment Campaign does not know.
    UnknownParentMoment,
    /// A candidate record differs from the record its frontier entry maps to.
    CandidateMismatch,
    /// The candidate's parent moment has no current restorable snapshot.
    Ineligible(IneligibleReason),
    /// The snapshot table already holds this moment, or the key is not a moment.
    InvalidSnapshotKey,
    /// A result names a selection that is not outstanding.
    UnknownSelection,
    /// A result reports the same child moment twice.
    DuplicateChild,
    /// A result reports the same finding twice.
    DuplicateFinding,
    /// The legacy configuration has no Campaign representation.
    DegenerateConfig(DegenerateConfig),
    /// A derived bound overflowed `u64`.
    BoundOverflow,
    /// A Campaign stop has no ChaosControl meaning.
    UnmappedStop,
    /// The journal mirror failed to decode or authenticate.
    Wire(WireError),
}

impl std::fmt::Display for AdapterError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "campaign adapter: {self:?}")
    }
}

impl std::error::Error for AdapterError {}

impl From<RankError> for AdapterError {
    fn from(error: RankError) -> Self {
        Self::Rank(error)
    }
}

impl From<campaign_core::CampaignError> for AdapterError {
    fn from(error: campaign_core::CampaignError) -> Self {
        Self::Campaign(error)
    }
}

impl From<campaign_choregraph::AdapterError> for AdapterError {
    fn from(error: campaign_choregraph::AdapterError) -> Self {
        Self::Choregraph(error)
    }
}

impl From<campaign_choregraph::HistoryError> for AdapterError {
    fn from(error: campaign_choregraph::HistoryError) -> Self {
        Self::History(error)
    }
}

impl From<WireError> for AdapterError {
    fn from(error: WireError) -> Self {
        Self::Wire(error)
    }
}

#[cfg(test)]
mod tests;
