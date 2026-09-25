//! Branch results as Campaign result observations.
//!
//! ChaosControl evaluates every branch itself. Only bounded child moments (branches
//! with new edges and a retained snapshot) and opaque finding references cross into
//! Campaign; coverage, assertions, schedules, and bug reports stay product-owned.

use std::collections::BTreeSet;

use campaign_core::{
    ChildObservation, FindingRef, FrameWriter, FrontierState, Id, IdKind, ResultObservation,
    SelectionDecision,
};

use super::identity::AdapterIdentity;
use super::moments::AdmittedMoment;
use super::AdapterError;

const FINDING_LABEL: &[u8] = b"chaoscontrol.finding-reference.v1";
const FINDING_PRIORITY_LABEL: &[u8] = b"chaoscontrol.finding-priority-facts.v1";
const RESULT_PAYLOAD_LABEL: &[u8] = b"chaoscontrol.round-result.v1";

/// One admitted child moment the result reports.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetainedChild {
    /// `ChildMoment` identity.
    pub moment_id: Id,
    /// `EventPayload` identity of the child moment event.
    pub event_payload_id: Id,
}

impl From<&AdmittedMoment> for RetainedChild {
    fn from(moment: &AdmittedMoment) -> Self {
        Self {
            moment_id: moment.moment_id,
            event_payload_id: moment.event_payload_id,
        }
    }
}

/// Product facts of one settled round.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResultFacts {
    /// Retained child moments in branch order.
    pub children: Vec<RetainedChild>,
    /// Opaque references to new deduplicated findings.
    pub findings: Vec<FindingRef>,
    /// Digest of the product round manifest.
    pub round_manifest_digest: [u8; 32],
}

/// Reserved child observation of one child moment for one selection.
///
/// # Errors
/// Returns a Campaign identity error for a non-child moment or payload.
pub fn child_observation(
    campaign_id: Id,
    selection_id: Id,
    child: RetainedChild,
) -> Result<ChildObservation, AdapterError> {
    Ok(ChildObservation {
        reservation_id: Id::absent(),
        child_moment_id: child.moment_id,
        discovery_id: Id::absent(),
        event_payload_id: child.event_payload_id,
    }
    .derive(campaign_id, selection_id)?)
}

/// Opaque finding reference from one deduplicated product finding.
#[must_use]
pub fn finding_ref(
    dedup_key: u64,
    assertion_fingerprint: [u8; 32],
    parent_moment_id: Id,
) -> FindingRef {
    FindingRef {
        finding_id: FrameWriter::new(IdKind::Finding, FINDING_LABEL)
            .u64(dedup_key)
            .bytes(&assertion_fingerprint)
            .id(parent_moment_id)
            .finish(IdKind::Finding),
        priority_facts_id: FrameWriter::new(IdKind::SourceFacts, FINDING_PRIORITY_LABEL)
            .u64(dedup_key)
            .bytes(&assertion_fingerprint)
            .finish(IdKind::SourceFacts),
    }
}

fn is_known_selection(state: &FrontierState, selection_id: Id) -> bool {
    state
        .outstanding
        .iter()
        .any(|outstanding| outstanding.selection_id == selection_id)
        || state
            .settled
            .iter()
            .any(|settled| settled.selection_id == selection_id)
}

/// Maps one settled round to the result observation of its selection.
///
/// Campaign alone decides whether the observation settles, retries, or conflicts.
///
/// # Errors
/// Returns `CrossedCampaign` for another campaign, `UnknownSelection` for a
/// selection the state never published, `DuplicateChild` for a repeated child
/// moment, or a Campaign identity error.
// r[impl chaoscontrol.campaign_policy.observations]
pub fn result_observation(
    identity: &AdapterIdentity,
    state: &FrontierState,
    selection: &SelectionDecision,
    facts: &ResultFacts,
) -> Result<ResultObservation, AdapterError> {
    if selection.campaign_id != state.admission.campaign_id {
        return Err(AdapterError::CrossedCampaign);
    }
    if !is_known_selection(state, selection.selection_id) {
        return Err(AdapterError::UnknownSelection);
    }
    let mut seen = BTreeSet::new();
    let mut children = Vec::with_capacity(facts.children.len());
    for child in &facts.children {
        if !seen.insert(child.moment_id) {
            return Err(AdapterError::DuplicateChild);
        }
        children.push(child_observation(
            selection.campaign_id,
            selection.selection_id,
            *child,
        )?);
    }
    let payload_id = FrameWriter::new(IdKind::EventPayload, RESULT_PAYLOAD_LABEL)
        .id(selection.selection_id)
        .bytes(&facts.round_manifest_digest)
        .finish(IdKind::EventPayload);
    Ok(ResultObservation {
        result_id: Id::absent(),
        campaign_id: selection.campaign_id,
        selection_id: selection.selection_id,
        parent_moment_id: selection.parent_moment_id,
        operation_id: selection.operation_id,
        adapter_id: identity.adapter_id,
        payload_id,
        children,
        findings: facts.findings.clone(),
        derived_records: Vec::new(),
    }
    .derive()?)
}
