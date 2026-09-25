//! Product stop and wait mapping.
//!
//! Campaign records only structural stop classes and bounded reason identities.
//! Plateau, finding, signal, and maximum-round meaning stays in ChaosControl, and a
//! wait is never reported as frontier or budget exhaustion. No stop claims that all
//! behavior was explored: every mapped Campaign stop acknowledges incomplete search.

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
