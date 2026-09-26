//! Campaign control session over real Choregraph history.
//!
//! The session owns the history graph, the control branch, the control payloads,
//! the Campaign state (advanced with `apply_control_event`), and the control
//! projection validated at the last committed segment. Every publication is
//! planned with `plan_control_event_publication` against the previously validated
//! projection (D41 fast-forward), and sealing a segment advances the projection
//! with `validate_control_advance` and requires it to equal the applied state.
//! Callers stage a clone, seal it, commit the journal segment, and only then
//! adopt the staged session, so uncommitted state is always dropped.
//!
//! Campaign's no-pipelining host obligation (`publication_fence.durable_base`):
//! the session is the only store of its graph and branch, and `publish` moves
//! the branch to the planned value in the same call that admits the draft, so
//! the graph never holds a draft whose branch update is pending. A journal
//! segment makes its drafts and branch moves durable together or not at all,
//! and a segment that fails to commit halts the backend before anything is
//! planned or published over it.

use super::FrontierError;
use crate::campaign_adapter::{
    advance_projection, load_projection, projection_binding, AdapterIdentity, AdmittedMoment,
    CONTROL_BRANCH_NAME,
};
use campaign_choregraph::{
    admit_control_root, plan_control_event_publication, BranchRef, ControlPayloadEnvelope,
    ControlProjectionRequest, ControlPublication, ControlPublicationRequest, EventId, HistoryGraph,
    HistoryLimits, ParentChoices, ValidatedControlProjection,
};
use campaign_core::{CampaignControlEvent, FrontierState, Id, ProjectionBinding};
use std::collections::BTreeMap;

/// Most parents of one admitted event: moments and control events have one.
pub(super) const HISTORY_MAX_PARENTS: u32 = 4;
/// Most metadata entries of one admitted event.
pub(super) const HISTORY_MAX_METADATA: u32 = 4;
/// Longest role, key, or branch name.
pub(super) const HISTORY_MAX_LABEL_BYTES: u32 = 64;
/// Branch generation right after the campaign admission is published.
const ADMITTED_GENERATION: u64 = 1;

/// History bounds of one epoch: `max_events` history events, of which at most
/// `max_projection_events` form the control lineage one projection holds.
///
/// # Errors
/// Returns `BoundOverflow` when a bound does not fit `u32`, or the Choregraph
/// error of an invalid policy.
pub(super) fn history_limits(
    max_events: u64,
    max_projection_events: u64,
) -> Result<HistoryLimits, FrontierError> {
    let max_events = u32::try_from(max_events).map_err(|_| FrontierError::BoundOverflow)?;
    let max_projection_events =
        u32::try_from(max_projection_events).map_err(|_| FrontierError::BoundOverflow)?;
    HistoryLimits::new(
        max_events,
        HISTORY_MAX_PARENTS,
        HISTORY_MAX_METADATA,
        HISTORY_MAX_LABEL_BYTES,
        max_projection_events,
    )
    .map_err(|error| FrontierError::Adapter(error.into()))
}

/// Live Campaign control session of one epoch.
#[derive(Clone)]
pub(super) struct ControlSession {
    identity: AdapterIdentity,
    maximum_events: u32,
    graph: HistoryGraph,
    branch: BranchRef,
    payloads: Vec<ControlPayloadEnvelope>,
    projection: ValidatedControlProjection,
    state: FrontierState,
    moment_events: BTreeMap<Id, EventId>,
}

impl ControlSession {
    /// Opens a session by publishing the campaign admission below the root moment.
    ///
    /// # Errors
    /// Returns the Choregraph or Campaign error of a rejected admission.
    pub(super) fn genesis(
        identity: AdapterIdentity,
        limits: HistoryLimits,
        root: &AdmittedMoment,
        admission: CampaignControlEvent,
    ) -> Result<Self, FrontierError> {
        let control = admit_control_root(
            &root.graph,
            admission,
            identity.projection_schema_id,
            identity.adapter_id,
            identity.control_domain_id,
        )?;
        let branch = BranchRef {
            name: CONTROL_BRANCH_NAME.to_owned(),
            head: control.envelope.history_event_id,
            generation: ADMITTED_GENERATION,
        };
        let graph = control.graph_after_admission;
        let payloads = vec![control.envelope];
        let choices = ParentChoices::new();
        let binding = projection_binding(&identity, &graph, &branch)?;
        let projection = load_projection(&ControlProjectionRequest {
            graph: &graph,
            branch: &branch,
            parent_choices: &choices,
            maximum_events: limits.max_projection_events,
            expected_binding: binding,
            payloads: &payloads,
        })?;
        let state = projection.state().clone();
        let mut moment_events = BTreeMap::new();
        moment_events.insert(root.moment_id, root.event);
        Ok(Self {
            identity,
            maximum_events: limits.max_projection_events,
            graph,
            branch,
            payloads,
            projection,
            state,
            moment_events,
        })
    }

    /// Current Campaign state (applied incrementally).
    pub(super) const fn state(&self) -> &FrontierState {
        &self.state
    }

    /// Projection validated at the last committed segment.
    pub(super) const fn projection(&self) -> &ValidatedControlProjection {
        &self.projection
    }

    /// Current history graph.
    pub(super) const fn graph(&self) -> &HistoryGraph {
        &self.graph
    }

    /// Control payloads in lineage order.
    pub(super) fn payloads(&self) -> &[ControlPayloadEnvelope] {
        &self.payloads
    }

    /// Choregraph event of one admitted moment.
    pub(super) fn moment_event(&self, moment_id: Id) -> Option<EventId> {
        self.moment_events.get(&moment_id).copied()
    }

    /// Projection binding re-derived from the current graph and branch.
    ///
    /// # Errors
    /// Returns the Choregraph error of a malformed binding.
    pub(super) fn binding(&self) -> Result<ProjectionBinding, FrontierError> {
        Ok(projection_binding(
            &self.identity,
            &self.graph,
            &self.branch,
        )?)
    }

    /// Adopts a moment admitted into the current graph.
    pub(super) fn admit_moment(&mut self, moment: AdmittedMoment) {
        self.moment_events.insert(moment.moment_id, moment.event);
        self.graph = moment.graph;
    }

    /// Adopts a recorded moment re-admitted under its claimed identity.
    pub(super) fn readmit_moment(&mut self, moment_id: Id, event: EventId, graph: HistoryGraph) {
        self.moment_events.insert(moment_id, event);
        self.graph = graph;
    }

    /// Plans and applies one control event against the current history.
    ///
    /// # Errors
    /// Returns the Choregraph error of a stale or non-fast-forward history, or the
    /// Campaign error replay would raise for the event.
    pub(super) fn publish(
        &mut self,
        event: CampaignControlEvent,
    ) -> Result<ControlPublication, FrontierError> {
        let choices = ParentChoices::new();
        let publication = plan_control_event_publication(ControlPublicationRequest {
            current: ControlProjectionRequest {
                graph: &self.graph,
                branch: &self.branch,
                parent_choices: &choices,
                maximum_events: self.maximum_events,
                expected_binding: self.binding()?,
                payloads: &self.payloads,
            },
            previous: &self.projection,
            control_event: event,
        })?;
        self.state =
            campaign_core::apply_control_event(self.state.clone(), publication.control_event())?;
        self.graph = publication.graph_after_admission().clone();
        self.branch = publication.branch_plan().after.clone();
        self.payloads.push(ControlPayloadEnvelope {
            history_event_id: publication.history_event_id(),
            control_event: publication.control_event().clone(),
        });
        Ok(publication)
    }

    /// Advances the validated projection to the current history and requires it
    /// to equal the incrementally applied state.
    ///
    /// # Errors
    /// Returns the Choregraph error of a non-fast-forward history, or
    /// `StateDivergence`.
    pub(super) fn seal(&mut self) -> Result<(), FrontierError> {
        let choices = ParentChoices::new();
        let projection = advance_projection(
            &self.projection,
            &ControlProjectionRequest {
                graph: &self.graph,
                branch: &self.branch,
                parent_choices: &choices,
                maximum_events: self.maximum_events,
                expected_binding: self.binding()?,
                payloads: &self.payloads,
            },
        )?;
        if projection.state() != &self.state {
            return Err(FrontierError::StateDivergence);
        }
        self.projection = projection;
        Ok(())
    }
}
