//! Product stop and wait mapping.
//!
//! Campaign records only structural stop classes and bounded reason identities.
//! Plateau, finding, signal, and maximum-round meaning stays in ChaosControl, and a
//! wait is never reported as frontier or budget exhaustion. No stop claims that all
//! behavior was explored: every mapped Campaign stop acknowledges incomplete search.
//!
//! Host control capacity: the Choregraph projection bound holds the control
//! lineage, the control branch advances one generation per control event, and
//! the shared Choregraph graph admits at most `max_events` events (child
//! moments and control events). Every validated projection exposes the reserve
//! its control history still owes (one settlement per outstanding selection, the
//! stop of an open campaign, and a pending draft counted as a selection) and the
//! graph room left beside it (D75). The adapter stops under Campaign's
//! adapter-owned [`campaign_choregraph::control_capacity_reason_id`] when the
//! lineage cannot hold one more round beside that reserve, or when the round's
//! appends (control events and child moments) do not fit the exposed graph room,
//! so an executed selection always settles in full and the stop still publishes.

use campaign_choregraph::ValidatedControlProjection;
use campaign_core::{DecisionPlan, NextDecision, SelectionDecision, StopReason, WaitReason};

use super::identity::{AdapterIdentity, AdapterReason};
use super::AdapterError;

/// ChaosControl stop classes with a Campaign representation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductStop {
    /// No retained candidate remains (`frontier_exhausted`).
    FrontierExhausted,
    /// Coverage stopped growing for `stale_round_limit` rounds (`coverage_plateau`).
    CoveragePlateau,
    /// A short run found a bug (`bug_found`); findings stay non-terminal in Campaign.
    ShortRunFinding,
    /// The operator interrupted the run (`interrupted`).
    Interrupted,
    /// `max_rounds` selections completed (`completed`).
    Completed,
    /// `max_rounds == 0`: the run completed without any selection.
    CompletedWithoutSelection,
    /// The host's Choregraph projection bound (or the control branch generation
    /// range) cannot hold one more round, its settlement, and the stop
    /// (`control_capacity_exhausted`).
    ControlCapacityExhausted,
}

/// Next step of one sequential exploration round.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoundDecision {
    /// Execute the published selection after the publication fence.
    Execute(Box<SelectionDecision>),
    /// Campaign waits; the sequential shell treats this as an invariant error.
    Wait(WaitReason),
    /// Campaign stops with this product meaning.
    Stop(ProductStop),
}

/// Campaign stop reason of one product stop.
#[must_use]
// r[impl chaoscontrol.campaign_policy.stop]
pub fn stop_reason(identity: &AdapterIdentity, stop: ProductStop) -> StopReason {
    let consumer = |reason| StopReason::ConsumerRequested {
        reason_id: identity.reason_id(reason),
    };
    match stop {
        ProductStop::FrontierExhausted => StopReason::FrontierExhausted,
        ProductStop::Completed => StopReason::SelectionLimitReached,
        ProductStop::CoveragePlateau => consumer(AdapterReason::CoveragePlateau),
        ProductStop::ShortRunFinding => consumer(AdapterReason::ShortRunFinding),
        ProductStop::Interrupted => consumer(AdapterReason::OperatorSignal),
        ProductStop::CompletedWithoutSelection => {
            consumer(AdapterReason::CompletedWithoutSelection)
        }
        ProductStop::ControlCapacityExhausted => StopReason::ConsumerRequested {
            reason_id: campaign_choregraph::control_capacity_reason_id(),
        },
    }
}

/// Product stop of one Campaign stop reason.
///
/// # Errors
/// Returns `UnmappedStop` for budget, policy, control-event, or foreign reasons.
pub fn product_stop(reason: &StopReason) -> Result<ProductStop, AdapterError> {
    match reason {
        StopReason::FrontierExhausted => Ok(ProductStop::FrontierExhausted),
        StopReason::SelectionLimitReached => Ok(ProductStop::Completed),
        StopReason::ConsumerRequested { reason_id }
            if *reason_id == campaign_choregraph::control_capacity_reason_id() =>
        {
            Ok(ProductStop::ControlCapacityExhausted)
        }
        StopReason::ConsumerRequested { reason_id } => match AdapterReason::from_id(*reason_id) {
            Some(AdapterReason::CoveragePlateau) => Ok(ProductStop::CoveragePlateau),
            Some(AdapterReason::ShortRunFinding) => Ok(ProductStop::ShortRunFinding),
            Some(AdapterReason::OperatorSignal) => Ok(ProductStop::Interrupted),
            Some(AdapterReason::CompletedWithoutSelection) => {
                Ok(ProductStop::CompletedWithoutSelection)
            }
            _ => Err(AdapterError::UnmappedStop),
        },
        StopReason::BudgetExhausted { .. }
        | StopReason::PolicyRequested { .. }
        | StopReason::ControlEventLimitReached => Err(AdapterError::UnmappedStop),
    }
}

/// Maps one Campaign decision plan to the next round step.
///
/// # Errors
/// Returns `CrossedIdentity` for a plan of another campaign and `UnmappedStop` for
/// a stop without product meaning or one that does not acknowledge incomplete search.
pub fn round_decision(
    identity: &AdapterIdentity,
    plan: DecisionPlan,
    campaign_id: campaign_core::Id,
) -> Result<RoundDecision, AdapterError> {
    if plan.campaign_id != campaign_id || plan.scheduling_policy_id != identity.scheduling_policy_id
    {
        return Err(AdapterError::CrossedIdentity);
    }
    match plan.decision {
        NextDecision::Select(selection) => Ok(RoundDecision::Execute(selection)),
        NextDecision::Wait(reason) => Ok(RoundDecision::Wait(reason)),
        NextDecision::Stop(stop) => {
            if !stop.incomplete_search_acknowledged {
                return Err(AdapterError::UnmappedStop);
            }
            Ok(RoundDecision::Stop(product_stop(&stop.reason)?))
        }
    }
}

/// Host control capacity of one validated control projection: the lineage and
/// Campaign's exposed reserve and graph room (D75, D83).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostCapacity {
    /// Control lineage events the projection holds.
    pub lineage: u64,
    /// Control events the projection's history still owes beyond its lineage
    /// (`ValidatedControlProjection::reserved_control_events`): settlements of
    /// outstanding selections, the stop of an open campaign, and each control
    /// draft descending from the branch head (its own event and settlement).
    pub reserved_control_events: u32,
    /// Choregraph projection bound.
    pub maximum_events: u32,
    /// Graph events other appends may still make without consuming the reserve
    /// (`ValidatedControlProjection::graph_room`). Campaign holds one campaign
    /// per graph and envelope (D82), so every other append is this shell's own.
    pub graph_room: u32,
    /// Control branch generation of the admission's binding.
    pub admitted_generation: u64,
}

impl HostCapacity {
    /// Host control capacity of `projection`.
    ///
    /// # Errors
    /// Returns `BoundOverflow` when the lineage length does not fit `u64`.
    pub fn of(projection: &ValidatedControlProjection) -> Result<Self, AdapterError> {
        Ok(Self {
            lineage: u64::try_from(projection.control_events().len())
                .map_err(|_| AdapterError::BoundOverflow)?,
            reserved_control_events: projection.reserved_control_events(),
            maximum_events: projection.maximum_events(),
            graph_room: projection.graph_room(),
            admitted_generation: projection.state().admission.projection.generation,
        })
    }
}

/// Most history one round adds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoundDemand {
    /// Control events: the selection and its whole settlement.
    pub control_events: u64,
    /// Graph events: the control events plus the round's child moments.
    pub history_events: u64,
}

/// Whether the host control capacity still holds one more round.
///
/// The lineage, Campaign's exposed control reserve (which already holds the
/// stop), and the round's control events must fit both the Choregraph
/// projection bound and the control branch generations left after the
/// admission's binding. Every graph append the round makes, its control events
/// and its child moments alike, must fit the exposed graph room, so the round
/// never consumes the reserve that keeps pending publications, outstanding
/// settlements, and the stop publishable. Campaign's own planner reserves one
/// settlement event per selection; a ChaosControl round settles with several
/// control events and admits its child moments first, so the shell checks the
/// whole round before it draws.
///
/// # Errors
/// Returns `BoundOverflow` when the demand does not fit `u64`.
pub fn round_fits_capacity(
    capacity: &HostCapacity,
    round: RoundDemand,
) -> Result<bool, AdapterError> {
    let lineage = capacity
        .lineage
        .checked_add(u64::from(capacity.reserved_control_events))
        .and_then(|owed| owed.checked_add(round.control_events))
        .ok_or(AdapterError::BoundOverflow)?;
    Ok(lineage <= u64::from(capacity.maximum_events)
        && capacity.admitted_generation.checked_add(lineage).is_some()
        && round.history_events <= u64::from(capacity.graph_room))
}

/// Whether a Choregraph publication or plan was refused for host control
/// capacity: the projection bound, the branch generation range, or the graph
/// event limit.
#[must_use]
pub const fn is_capacity_refusal(error: &AdapterError) -> bool {
    matches!(
        error,
        AdapterError::Choregraph(
            campaign_choregraph::AdapterError::ProjectionCapacityExhausted
                | campaign_choregraph::AdapterError::GenerationCapacityExhausted
                | campaign_choregraph::AdapterError::GraphCapacityExhausted
        )
    )
}
