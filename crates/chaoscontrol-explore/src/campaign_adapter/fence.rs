//! Shell points that must delegate to Campaign: projection loading, cache
//! validation, the publication execution gate, and receipt claims.
//!
//! The functions are pure; the durable journal supplies the history, cache, and
//! publication evidence they check.

use campaign_choregraph::{
    can_execute_selection_after_publication, validate_cache_against_history,
    validate_control_projection, AdapterResult, CacheValidationRequest, ControlProjectionRequest,
    ControlPublication, DurablePublicationEvidence, ValidatedControlProjection,
};
use campaign_core::{CacheProjection, FrontierState, ReceiptClaims, SelectionDecision};

/// Loads the control projection of exactly the presented history.
///
/// # Errors
/// Returns the Choregraph adapter error of a stale, rewritten, forged, or ambiguous history.
pub fn load_projection(
    request: &ControlProjectionRequest<'_>,
) -> AdapterResult<ValidatedControlProjection> {
    validate_control_projection(request)
}

/// Validates a derived cache against authoritative history; the journal wins.
///
/// # Errors
/// Returns the Choregraph adapter error of a stale or incomplete cache.
pub fn validate_cache(
    request: &CacheValidationRequest<'_>,
    cache: &CacheProjection,
) -> AdapterResult<FrontierState> {
    validate_cache_against_history(request, cache)
}

/// Grants execution only for the exact selection a durable publication published.
///
/// A missing publication or missing durability evidence refuses execution.
#[must_use]
// r[impl chaoscontrol.campaign_policy.publication_fence]
pub fn execution_authority(
    selection: &SelectionDecision,
    publication: Option<&ControlPublication>,
    evidence: Option<&DurablePublicationEvidence>,
) -> bool {
    let (Some(publication), Some(evidence)) = (publication, evidence) else {
        return false;
    };
    can_execute_selection_after_publication(selection, publication, evidence) == Ok(true)
}

/// Promotion claims of any Campaign receipt: always none.
///
/// Rank, selection, exit status, and check references never become VM, fault,
/// assertion, finding, replay, minimization, or release evidence.
#[must_use]
// r[impl chaoscontrol.campaign_policy.evidence]
pub fn receipt_claims() -> ReceiptClaims {
    ReceiptClaims::no_claims()
}
