//! Campaign bounds, selection profile, admission, and request construction from
//! `ExplorerConfig`.
//!
//! Legacy configurations without a Campaign representation are typed
//! pre-exploration errors: `max_frontier == 0`, a non-finite or negative
//! `rare_edge_weight`, and a missing restorable bootstrap root. With a restorable
//! root and `max_frontier >= 1` the retained frontier never empties (candidates are
//! reusable and pruning keeps at least one), so the legacy empty-frontier live-controller
//! continuation and corpus recycling are unreachable; an empty Campaign frontier maps
//! to `FrontierExhausted`. `max_rounds == 0` admits no selection and stops with
//! `ConsumerRequested{completed-without-selection}`.

use campaign_core::{
    AdmissionRule, CampaignAdmission, DuplicateCandidateRule, EntropyTicket, ExplorationBounds,
    FrameWriter, FrontierState, Id, IdKind, PolicyImplementation, ProjectionBinding,
    ResourceAmount, RetentionPlanRequest, RootMoment, SelectionCountDecay, SelectionProfile,
    SelectionRequest, StoppingRule, CAMPAIGN_SCHEMA_VERSION, UNIT_COST,
};
use chaoscontrol_protocol::admission::MAX_ASSERTION_REPORT_ENTRIES;
use chaoscontrol_protocol::branch_marker::MAX_MARKERS_PER_RUN;

use super::entropy::{entropy_ticket, selection_mode, ticket_counter, EntropyDraws};
use super::identity::{AdapterIdentity, AdapterReason};
use super::moments::AdmittedMoment;
use super::ranks::{
    INSERTION_ORDER_DIMENSION, MAX_RANK_MAGNITUDE, ORIGINAL_SCORE_DIMENSION, RANK_DIMENSIONS,
};
use super::stops::RoundDemand;
use super::AdapterError;
use crate::explorer::ExplorerConfig;

/// Retention order: original score, then insertion order (legacy stable truncation).
pub const RETENTION_RANK_ORDER: [u32; 2] = [ORIGINAL_SCORE_DIMENSION, INSERTION_ORDER_DIMENSION];
/// The bootstrap snapshot is the only root moment.
pub const ROOT_MOMENTS: u64 = 1;
/// Sequential shell: one outstanding selection, settled before the next plan.
pub const SEQUENTIAL_OUTSTANDING: u64 = 1;
/// Frontier entries one branch can push: one per marker observation, else one.
pub const ENTRIES_PER_BRANCH: u64 = MAX_MARKERS_PER_RUN as u64;
/// Findings one branch can report: one per failed structured assertion.
pub const FINDINGS_PER_BRANCH: u64 = MAX_ASSERTION_REPORT_ENTRIES as u64;
/// Genesis control events besides root guidance: admission, root candidate
/// batch, and the root prune to `max_frontier`.
pub const GENESIS_CONTROL_EVENTS: u64 = 3;
/// Per-round control events besides guidance: selection, result or closure,
/// candidate batch, decay revision, prune, and an ineligibility retirement.
pub const ROUND_CONTROL_EVENTS: u64 = 6;
/// Closing control events: one stop.
pub const CLOSING_CONTROL_EVENTS: u64 = 1;
/// The adapter emits no derived records; Campaign bounds are at least one.
pub const DERIVED_RECORDS: u64 = 1;
/// One resource dimension: branch-selection units.
pub const RESOURCE_DIMENSIONS: u64 = 1;
/// One operation schema: frontier expansion.
pub const OPERATION_SCHEMAS: u64 = 1;
const REQUEST_LABEL: &[u8] = b"chaoscontrol.selection-request.v1";

/// Legacy configurations the Campaign policy rejects before exploration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DegenerateConfig {
    /// `max_frontier == 0`: Campaign retains at least one candidate.
    ZeroMaxFrontier,
    /// `rare_edge_weight` is non-finite or negative, so scores cannot rank.
    InvalidRareEdgeWeight,
    /// No restorable bootstrap root: legacy would continue from the live controller.
    MissingRestorableRoot,
}

/// Bootstrap facts that decide whether exploration can start.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BootstrapFacts {
    /// The bootstrap produced a restorable snapshot for the root moment.
    pub restorable_root: bool,
}

/// How many selections the run may publish.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionLimit {
    /// Up to `max_rounds` selections; completion is `SelectionLimitReached`.
    Rounds(u64),
    /// `max_rounds == 0`: stop with `completed-without-selection` before planning.
    NoSelection,
}

/// Admitted Campaign inputs derived from one `ExplorerConfig`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignProfile {
    /// Derived exploration bounds.
    pub bounds: ExplorationBounds,
    /// Derived selection profile (exploratory, no shared decay, no finding limit).
    pub selection_profile: SelectionProfile,
    /// Declared candidate admission rule.
    pub admission_rule: AdmissionRule,
    /// Branch-selection budget.
    pub resource_budget: ResourceAmount,
    /// Selection limit.
    pub selection_limit: SelectionLimit,
}

impl CampaignProfile {
    /// Most control events one round publishes: `ROUND_CONTROL_EVENTS` plus one
    /// guidance record per entry of the largest candidate batch. The bounds budget
    /// `max_control_events` with the same per-round figure.
    ///
    /// # Errors
    /// Returns `BoundOverflow` when the count does not fit `u64`.
    pub fn round_control_events(&self) -> Result<u64, AdapterError> {
        checked_add(ROUND_CONTROL_EVENTS, self.bounds.max_candidate_batch)
    }

    /// Most history one round adds: [`CampaignProfile::round_control_events`]
    /// control events, plus one child moment per branch (`max_child_moments`)
    /// in the shared graph.
    ///
    /// # Errors
    /// Returns `BoundOverflow` when a count does not fit `u64`.
    pub fn round_demand(&self) -> Result<RoundDemand, AdapterError> {
        let control_events = self.round_control_events()?;
        Ok(RoundDemand {
            control_events,
            history_events: checked_add(control_events, self.bounds.max_child_moments)?,
        })
    }
}

fn checked_mul(left: u64, right: u64) -> Result<u64, AdapterError> {
    left.checked_mul(right).ok_or(AdapterError::BoundOverflow)
}

fn checked_add(left: u64, right: u64) -> Result<u64, AdapterError> {
    left.checked_add(right).ok_or(AdapterError::BoundOverflow)
}

fn reject_degenerate(
    config: &ExplorerConfig,
    bootstrap: BootstrapFacts,
) -> Result<(), AdapterError> {
    if config.max_frontier == 0 {
        return Err(AdapterError::DegenerateConfig(
            DegenerateConfig::ZeroMaxFrontier,
        ));
    }
    if !config.rare_edge_weight.is_finite() || config.rare_edge_weight < 0.0 {
        return Err(AdapterError::DegenerateConfig(
            DegenerateConfig::InvalidRareEdgeWeight,
        ));
    }
    if !bootstrap.restorable_root {
        return Err(AdapterError::DegenerateConfig(
            DegenerateConfig::MissingRestorableRoot,
        ));
    }
    Ok(())
}

fn exploration_bounds(config: &ExplorerConfig) -> Result<ExplorationBounds, AdapterError> {
    let rounds = config.max_rounds.max(1);
    let branches = u64::try_from(config.branch_factor)
        .map_err(|_| AdapterError::BoundOverflow)?
        .max(1);
    let max_frontier =
        u64::try_from(config.max_frontier).map_err(|_| AdapterError::BoundOverflow)?;
    let branch_entries = checked_mul(branches, ENTRIES_PER_BRANCH)?;
    let max_candidates = checked_add(ROOT_MOMENTS, checked_mul(rounds, branch_entries)?)?;
    if i64::try_from(max_candidates).is_err() {
        return Err(AdapterError::BoundOverflow);
    }
    let per_round_events = checked_add(ROUND_CONTROL_EVENTS, branch_entries)?;
    // Root guidance: one record per root proposal (one per bootstrap marker).
    let genesis_events = checked_add(GENESIS_CONTROL_EVENTS, ENTRIES_PER_BRANCH)?;
    let max_control_events = checked_add(
        checked_add(genesis_events, CLOSING_CONTROL_EVENTS)?,
        checked_mul(rounds, per_round_events)?,
    )?;
    let rank_dimensions =
        u64::try_from(RANK_DIMENSIONS).map_err(|_| AdapterError::BoundOverflow)?;
    Ok(ExplorationBounds {
        bounds_id: Id::absent(),
        max_roots: ROOT_MOMENTS,
        max_candidates,
        max_candidate_batch: branch_entries,
        max_retained_candidates: max_frontier,
        max_selections: rounds,
        max_guidance_records: checked_add(max_candidates, rounds)?,
        max_outstanding: SEQUENTIAL_OUTSTANDING,
        max_depth: checked_add(rounds, 1)?,
        max_child_moments: branches,
        max_derived_records: DERIVED_RECORDS,
        max_rank_dimensions: rank_dimensions,
        max_rank_magnitude: MAX_RANK_MAGNITUDE,
        max_findings: checked_mul(checked_mul(rounds, branches)?, FINDINGS_PER_BRANCH)?,
        max_control_events,
        max_resource_dimensions: RESOURCE_DIMENSIONS,
        max_operation_schemas: OPERATION_SCHEMAS,
        max_prune_batch: branch_entries,
    }
    .derive()?)
}

/// Admits one exploration configuration under the Campaign policy.
///
/// # Errors
/// Returns `DegenerateConfig` for configurations without a Campaign representation,
/// `BoundOverflow` for bounds that do not fit, or a Campaign derivation error.
// r[impl chaoscontrol.campaign_policy.adapter]
pub fn admit_config(
    identity: &AdapterIdentity,
    config: &ExplorerConfig,
    bootstrap: BootstrapFacts,
) -> Result<CampaignProfile, AdapterError> {
    reject_degenerate(config, bootstrap)?;
    let bounds = exploration_bounds(config)?;
    let admission_rule = AdmissionRule {
        duplicate_candidate: DuplicateCandidateRule::Reject,
    };
    let selection_profile = SelectionProfile {
        profile_id: Id::absent(),
        scheduling_policy_id: identity.scheduling_policy_id,
        admission_policy_id: admission_rule.admission_policy_id()?,
        coalescing_policy_id: identity.coalescing_policy_id,
        mode: selection_mode(identity),
        decay: SelectionCountDecay::None,
        stopping: StoppingRule {
            finding_limit: None,
        },
        implementation: PolicyImplementation::ProductionCompiled {
            policy_id: identity.production_policy_id,
        },
    }
    .derive()?;
    let selection_limit = match config.max_rounds {
        0 => SelectionLimit::NoSelection,
        rounds => SelectionLimit::Rounds(rounds),
    };
    Ok(CampaignProfile {
        resource_budget: ResourceAmount {
            dimension_id: identity.resource_dimension_id,
            amount: checked_mul(bounds.max_selections, UNIT_COST)?,
        },
        bounds,
        selection_profile,
        admission_rule,
        selection_limit,
    })
}

/// Campaign admission bound to the root moment and the pre-admission projection.
#[must_use]
pub fn campaign_admission(
    identity: &AdapterIdentity,
    profile: &CampaignProfile,
    root: &AdmittedMoment,
    projection: ProjectionBinding,
) -> CampaignAdmission {
    CampaignAdmission {
        campaign_id: Id::absent(),
        schema_version: CAMPAIGN_SCHEMA_VERSION,
        policy_id: identity.policy_id,
        guidance_profile_id: identity.guidance_profile_id,
        entropy_profile_id: identity.entropy_source_id,
        budget_profile_id: profile.bounds.bounds_id,
        adapter_id: identity.adapter_id,
        control_domain_id: identity.control_domain_id,
        bounds: profile.bounds.clone(),
        resource_budgets: vec![profile.resource_budget.clone()],
        roots: vec![RootMoment {
            moment_id: root.moment_id,
            projection_id: projection.projection_id,
            adapter_id: identity.adapter_id,
        }],
        operation_schema_ids: vec![identity.operation_schema_id],
        projection,
    }
}

/// Selection request for the current state, projection, and recorded draws.
///
/// # Errors
/// Returns a Campaign error for an exhausted counter or crossed identities.
pub fn selection_request(
    identity: &AdapterIdentity,
    profile: &CampaignProfile,
    state: &FrontierState,
    projection: &ProjectionBinding,
    draws: EntropyDraws,
) -> Result<SelectionRequest, AdapterError> {
    let counter = ticket_counter(state)?;
    let ticket: EntropyTicket = entropy_ticket(
        identity,
        state.admission().campaign_id,
        state.state_id(),
        profile.selection_profile.profile_id,
        counter,
        draws,
    )?;
    Ok(SelectionRequest {
        request_id: FrameWriter::new(IdKind::Request, REQUEST_LABEL)
            .id(state.state_id())
            .u64(counter)
            .finish(IdKind::Request),
        campaign_id: state.admission().campaign_id,
        state_id: state.state_id(),
        profile: profile.selection_profile.clone(),
        entropy: Some(ticket),
        expected_generation: projection.generation,
        expected_head_id: projection.head_id,
    })
}

/// Retention request pruning the frontier to `max_frontier` by original score,
/// then insertion order.
#[must_use]
pub fn retention_plan_request(
    identity: &AdapterIdentity,
    state: &FrontierState,
) -> RetentionPlanRequest {
    RetentionPlanRequest {
        campaign_id: state.admission().campaign_id,
        state_id: state.state_id(),
        retention_profile_id: identity.retention_profile_id,
        rank_order: RETENTION_RANK_ORDER.to_vec(),
        reason_id: identity.reason_id(AdapterReason::RetentionPrune),
    }
}
