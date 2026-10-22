//! Positive and negative tests of the pure Campaign adapter.
//!
//! `Session` drives real Campaign and Choregraph values the way the future
//! Explorer shell will, without a journal: every control event is planned through
//! `plan_control_event_publication` against the current graph and branch and then
//! applied with `apply_control_event`.

mod candidates;
mod conformance;
mod eligibility;
mod entropy;
mod fence;
mod identity;
mod moments;
mod profile;
mod ranks;
mod results;
mod stops;
mod wire;

use std::collections::BTreeMap;

use campaign_choregraph::{
    admit_control_root, plan_control_event_publication, BranchRef, ControlPayloadEnvelope,
    ControlProjectionRequest, ControlPublication, ControlPublicationRequest, HistoryGraph,
    HistoryLimits, ParentChoices, ValidatedControlProjection,
};
use campaign_core::{
    CampaignControlEvent, DecisionPlan, FrontierState, Id, ProjectionBinding, ResultObservation,
    SelectionDecision, SelectionEvent,
};

use crate::campaign_adapter::{
    admit_child_moment, admit_config, admit_root_moment, campaign_admission, candidate_record,
    decay_revision, generation_profile_id, guidance_record, load_projection, projection_binding,
    retention_plan_request, root_control_branch, round_decision, selection_request,
    AdapterIdentity, AdmittedMoment, BootstrapFacts, BranchSlot, CampaignProfile, ChildMomentFacts,
    EntropyDraws, FrontierEntryFacts, GuidanceFacts, LegacyDraws, ResultFacts, RootMomentFacts,
    RoundDecision,
};
use crate::explorer::{ExplorationMode, ExplorerConfig};

/// Test exploration seed.
pub(super) const SEED: u64 = 42;
/// Test round bound.
pub(super) const ROUNDS: u64 = 4;
/// Test frontier bound.
pub(super) const MAX_FRONTIER: usize = 3;
/// Test branch factor.
pub(super) const BRANCHES: usize = 2;
/// Test graph event limit and projection bound.
pub(super) const HISTORY_MAX_EVENTS: u32 = 512;
const HISTORY_MAX_PARENTS: u32 = 4;
const HISTORY_MAX_METADATA: u32 = 4;
const HISTORY_MAX_LABEL_BYTES: u32 = 64;
const ADMITTED_GENERATION: u64 = 1;
/// A unit draw on the exploit branch (`u >= 0.1`).
pub(super) const EXPLOIT: LegacyDraws = LegacyDraws {
    unit: 0.5,
    index: None,
};

/// Explore draw selecting legacy vector position `index`.
pub(super) const fn explore(index: usize) -> LegacyDraws {
    LegacyDraws {
        unit: 0.0,
        index: Some(index),
    }
}

pub(super) fn digest(byte: u8) -> [u8; 32] {
    [byte; 32]
}

pub(super) fn config() -> ExplorerConfig {
    ExplorerConfig {
        seed: SEED,
        max_rounds: ROUNDS,
        max_frontier: MAX_FRONTIER,
        branch_factor: BRANCHES,
        ..ExplorerConfig::default()
    }
}

pub(super) fn root_facts() -> RootMomentFacts {
    RootMomentFacts {
        epoch: 0,
        config_digest: digest(1),
        seed: SEED,
        bootstrap_tick: 7,
        coverage_digest: digest(2),
        oracle_digest: digest(3),
        base_set_id: digest(4),
    }
}

pub(super) fn empty_graph() -> HistoryGraph {
    empty_graph_within(HISTORY_MAX_EVENTS)
}

/// Empty graph admitting at most `max_events` events; the projection bound stays
/// [`HISTORY_MAX_EVENTS`].
pub(super) fn empty_graph_within(max_events: u32) -> HistoryGraph {
    HistoryGraph::new(
        HistoryLimits::new(
            max_events,
            HISTORY_MAX_PARENTS,
            HISTORY_MAX_METADATA,
            HISTORY_MAX_LABEL_BYTES,
            HISTORY_MAX_EVENTS,
        )
        .unwrap(),
    )
    .unwrap()
}

/// Product bookkeeping of one admitted frontier entry.
#[derive(Clone, Copy, Debug)]
pub(super) struct Entry {
    pub seq: u64,
    pub score: f64,
    pub times_selected: u32,
}

/// Live Campaign session over real Choregraph history.
pub(super) struct Session {
    pub identity: AdapterIdentity,
    pub profile: CampaignProfile,
    pub graph: HistoryGraph,
    pub branch: BranchRef,
    pub payloads: Vec<ControlPayloadEnvelope>,
    pub state: FrontierState,
    pub root: AdmittedMoment,
    pub generation_profile_id: Id,
    pub entries: BTreeMap<Id, Entry>,
    pub moment_events: BTreeMap<Id, campaign_choregraph::EventId>,
    next_seq: u64,
}

impl Session {
    pub fn open(config: &ExplorerConfig) -> Self {
        Self::open_within_graph(config, HISTORY_MAX_EVENTS)
    }

    /// [`Session::open`] over a graph admitting at most `graph_max_events` events.
    pub fn open_within_graph(config: &ExplorerConfig, graph_max_events: u32) -> Self {
        let identity = AdapterIdentity::for_seed(config.seed);
        let profile = admit_config(
            &identity,
            config,
            BootstrapFacts {
                restorable_root: true,
            },
        )
        .unwrap();
        let root = admit_root_moment(
            &empty_graph_within(graph_max_events),
            &identity,
            &root_facts(),
        )
        .unwrap();
        let branch = root_control_branch(&root);
        let binding = projection_binding(&identity, &root.graph, &branch).unwrap();
        let admission =
            campaign_core::admit_campaign(campaign_admission(&identity, &profile, &root, binding))
                .unwrap();
        let control = admit_control_root(
            &root.graph,
            admission.clone(),
            identity.projection_schema_id,
            identity.adapter_id,
            identity.control_domain_id,
        )
        .unwrap();
        let state = campaign_core::reconstruct(std::slice::from_ref(&admission)).unwrap();
        let mut moment_events = BTreeMap::new();
        moment_events.insert(root.moment_id, root.event);
        Self {
            generation_profile_id: generation_profile_id(config).unwrap(),
            graph: control.graph_after_admission,
            branch: BranchRef {
                head: control.envelope.history_event_id,
                generation: ADMITTED_GENERATION,
                ..branch
            },
            payloads: vec![control.envelope],
            state,
            root,
            identity,
            profile,
            entries: BTreeMap::new(),
            moment_events,
            next_seq: 0,
        }
    }

    pub fn campaign_id(&self) -> Id {
        self.state.admission().campaign_id
    }

    pub fn binding(&self) -> ProjectionBinding {
        projection_binding(&self.identity, &self.graph, &self.branch).unwrap()
    }

    pub fn control_events(&self) -> Vec<CampaignControlEvent> {
        self.payloads
            .iter()
            .map(|payload| payload.control_event.clone())
            .collect()
    }

    /// Cold-start projection of the current history (`validate_control_projection`).
    pub fn projection(&self) -> ValidatedControlProjection {
        self.projection_within(HISTORY_MAX_EVENTS)
    }

    /// Cold-start projection of the current history under the host projection
    /// bound `maximum_events`.
    pub fn projection_within(&self, maximum_events: u32) -> ValidatedControlProjection {
        let choices = ParentChoices::new();
        load_projection(&ControlProjectionRequest {
            graph: &self.graph,
            branch: &self.branch,
            parent_choices: &choices,
            maximum_events,
            expected_binding: self.binding(),
            payloads: &self.payloads,
        })
        .unwrap()
    }

    /// Plans `event` through the cold-start projection of the current history: the
    /// session holds no projection, so it validates one and publishes through it.
    pub fn plan_publication(&self, event: CampaignControlEvent) -> ControlPublication {
        self.publication_within(event, HISTORY_MAX_EVENTS).unwrap()
    }

    /// [`Session::plan_publication`] under the host projection bound `maximum_events`.
    pub fn publication_within(
        &self,
        event: CampaignControlEvent,
        maximum_events: u32,
    ) -> campaign_choregraph::AdapterResult<ControlPublication> {
        let choices = ParentChoices::new();
        let previous = self.projection_within(maximum_events);
        plan_control_event_publication(ControlPublicationRequest {
            current: ControlProjectionRequest {
                graph: &self.graph,
                branch: &self.branch,
                parent_choices: &choices,
                maximum_events,
                expected_binding: self.binding(),
                payloads: &self.payloads,
            },
            previous: &previous,
            control_event: event,
        })
    }

    pub fn publish(&mut self, event: CampaignControlEvent) -> ControlPublication {
        let publication = self.plan_publication(event);
        self.state =
            campaign_core::apply_control_event(self.state.clone(), publication.control_event())
                .unwrap();
        self.graph = publication.graph_after_admission().clone();
        self.branch = publication.branch_plan().after.clone();
        self.payloads.push(ControlPayloadEnvelope {
            history_event_id: publication.history_event_id(),
            control_event: publication.control_event().clone(),
        });
        publication
    }

    pub fn entry_facts(&self, parent_moment_id: Id, seq: u64) -> FrontierEntryFacts {
        FrontierEntryFacts {
            parent_moment_id,
            mode: ExplorationMode::FaultSchedule,
            entry_seq: seq,
            marker_digest: None,
        }
    }

    /// Admits one batch of frontier entries and their guidance; returns candidate ids.
    pub fn admit_entries(&mut self, parent_moment_id: Id, scores: &[f64]) -> Vec<Id> {
        let mut records = Vec::with_capacity(scores.len());
        let mut admitted = Vec::with_capacity(scores.len());
        for score in scores {
            let seq = self.next_seq;
            self.next_seq += 1;
            let facts = self.entry_facts(parent_moment_id, seq);
            let record = candidate_record(
                &self.identity,
                &self.state,
                &facts,
                self.generation_profile_id,
            )
            .unwrap();
            self.entries.insert(
                record.candidate_id,
                Entry {
                    seq,
                    score: *score,
                    times_selected: 0,
                },
            );
            admitted.push(record.candidate_id);
            records.push(record);
        }
        let batch = campaign_core::admit_candidates(
            &self.state,
            records,
            self.profile.admission_rule,
            self.identity.coalescing_policy_id,
        )
        .unwrap();
        self.publish(batch);
        for candidate_id in &admitted {
            let entry = self.entries[candidate_id];
            let guidance = guidance_record(
                &self.identity,
                self.campaign_id(),
                *candidate_id,
                GuidanceFacts {
                    score: entry.score,
                    times_selected: entry.times_selected,
                    entry_seq: entry.seq,
                },
            )
            .unwrap();
            let event = campaign_core::admit_guidance(&self.state, guidance).unwrap();
            self.publish(event);
        }
        admitted
    }

    pub fn retained(&self) -> usize {
        self.state
            .candidates()
            .iter()
            .filter(|candidate| {
                matches!(candidate.status, campaign_core::CandidateStatus::Retained)
            })
            .count()
    }

    pub fn request(&self, draws: LegacyDraws) -> campaign_core::SelectionRequest {
        let draws = EntropyDraws::from_legacy(draws, self.retained()).unwrap();
        selection_request(
            &self.identity,
            &self.profile,
            &self.state,
            &self.binding(),
            draws,
        )
        .unwrap()
    }

    pub fn plan(&self, draws: LegacyDraws) -> DecisionPlan {
        campaign_core::plan_next(&self.state, self.request(draws), &self.binding()).unwrap()
    }

    /// Plans one round and publishes an executable selection.
    pub fn select(&mut self, draws: LegacyDraws) -> RoundDecision {
        let request = self.request(draws);
        let plan = campaign_core::plan_next(&self.state, request.clone(), &self.binding()).unwrap();
        let decision = round_decision(&self.identity, plan, self.campaign_id()).unwrap();
        if let RoundDecision::Execute(selection) = &decision {
            let event =
                SelectionEvent::new(selection.as_ref().clone(), request.profile, request.entropy)
                    .unwrap();
            self.publish(CampaignControlEvent::SelectionPublished(event));
        }
        decision
    }

    pub fn executed(&mut self, draws: LegacyDraws) -> SelectionDecision {
        match self.select(draws) {
            RoundDecision::Execute(selection) => *selection,
            other => panic!("expected a selection, got {other:?}"),
        }
    }

    pub fn child_facts(&self, selection: &SelectionDecision, slot: u32) -> ChildMomentFacts {
        ChildMomentFacts {
            campaign_id: self.campaign_id(),
            selection_id: selection.selection_id,
            operation_id: selection.operation_id,
            branch_slot: BranchSlot::Variant(slot),
            schedule_digest: digest(10),
            variant_digest: digest(11),
            coverage_digest: digest(12),
            oracle_digest: digest(13),
            total_ticks: 100,
            snapshot_tick: 100,
            base_set_id: digest(4),
        }
    }

    pub fn admit_child(&mut self, selection: &SelectionDecision, slot: u32) -> AdmittedMoment {
        let parent = self.moment_events[&selection.parent_moment_id];
        let facts = self.child_facts(selection, slot);
        let child = admit_child_moment(
            &self.graph,
            &self.identity,
            self.campaign_id(),
            &facts,
            parent,
        )
        .unwrap();
        self.graph = child.graph.clone();
        self.moment_events.insert(child.moment_id, child.event);
        child
    }

    /// Settles a selection, then applies the explicit division-decay revision.
    pub fn settle(
        &mut self,
        selection: &SelectionDecision,
        facts: &ResultFacts,
    ) -> ResultObservation {
        let observation = crate::campaign_adapter::result_observation(
            &self.identity,
            &self.state,
            selection,
            facts,
        )
        .unwrap();
        let event = campaign_core::admit_result(&self.state, observation.clone()).unwrap();
        self.publish(event);
        self.revise(selection.candidate_id);
        observation
    }

    pub fn revise(&mut self, candidate_id: Id) {
        let entry = self.entries.get_mut(&candidate_id).unwrap();
        entry.times_selected += 1;
        let entry = *entry;
        let latest = self
            .state
            .candidate(candidate_id)
            .unwrap()
            .latest_guidance
            .clone()
            .unwrap();
        let request = decay_revision(
            &self.identity,
            &latest,
            GuidanceFacts {
                score: entry.score,
                times_selected: entry.times_selected,
                entry_seq: entry.seq,
            },
        )
        .unwrap();
        let event = campaign_core::revise_guidance(&self.state, request).unwrap();
        self.publish(event);
    }

    /// Plans and publishes one retention prune; returns the pruned candidates.
    pub fn prune(&mut self) -> Vec<Id> {
        let request = retention_plan_request(&self.identity, &self.state);
        let Some(prune) = campaign_core::plan_pruning(&self.state, request).unwrap() else {
            return Vec::new();
        };
        let pruned = prune.candidates.clone();
        let event = campaign_core::prune_frontier(&self.state, prune).unwrap();
        self.publish(event);
        pruned
    }

    pub fn seq_of(&self, candidate_id: Id) -> u64 {
        self.entries[&candidate_id].seq
    }
}

/// Empty round result.
pub(super) fn no_children() -> ResultFacts {
    ResultFacts {
        children: Vec::new(),
        findings: Vec::new(),
        round_manifest_digest: digest(20),
    }
}
