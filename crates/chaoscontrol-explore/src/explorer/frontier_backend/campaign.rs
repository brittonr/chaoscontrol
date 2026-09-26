//! Campaign-backed candidate frontier with a durable selection journal.
//!
//! Journal segments per epoch (`output_dir/campaign-journal/epoch-NNNN/`):
//!
//! - closing segment of a recovered epoch: unobserved closure of every
//!   outstanding selection plus a `resume-epoch-closed` stop;
//! - genesis segment: epoch header, root moment, campaign admission, root
//!   candidates, their guidance, and a prune to `max_frontier` when the
//!   bootstrap offered more proposals;
//! - per round, the selection segment (selection facts + `SelectionPublished`),
//!   committed before any expansion effect, then the settlement segment (child
//!   moments, result, candidates, guidance, decay revision, prune, round facts),
//!   committed before the shell writes `checkpoint.json`;
//! - an eligibility segment when a candidate's snapshot became unrestorable
//!   (retirement, or unobserved closure plus retirement after publication);
//! - a stop segment.
//!
//! Durability classes: a committed segment advances the backend; an uncommitted
//! one drops the staged state and halts the backend; unknown durability poisons
//! the journal and halts; a sequence collision on the selection segment
//! resynchronizes and replans once with the round's recorded draws.

mod round;

use super::digests::config_digest;
use super::records::{
    encode, replay_epoch, EntryRecord, GenesisRecord, CONTROL, ENTRY, GENESIS, GENESIS_SCHEMA,
    MOMENT,
};
use super::session::{history_limits, ControlSession};
use super::{
    FrontierBackend, FrontierError, RetainedEntry, RootAdmission, RoundBegin, RoundContext,
    SelectionUse,
};
use crate::campaign_adapter::{
    admit_config, admit_root_moment, campaign_admission, candidate_record, encode_control_event,
    encode_moment, generation_profile_id, guidance_record, marker_digest, projection_binding,
    root_control_branch, root_moment_draft, stop_reason, unobserved_closure, AdapterError,
    AdapterIdentity, AdapterReason, BootstrapFacts, CampaignProfile, FrontierEntryFacts,
    GuidanceFacts, ProductStop, RootMomentFacts, SelectionLimit, SnapshotContext, SnapshotEntry,
    SnapshotTable,
};
use crate::checkpoint::CampaignJournalBinding;
use crate::explorer::journal::{
    CampaignJournal, CommittedSegment, JournalError, JournalRecord, PublicationIo,
    SystemPublicationIo,
};
use crate::explorer::ExplorerConfig;
use campaign_choregraph::HistoryGraph;
use campaign_core::{CampaignControlEvent, CandidateStatus, Id, StopReason};
use chaoscontrol_fault::schedule::FaultSchedule;
use chaoscontrol_vmm::controller::SimulationSnapshot;
use rand::RngCore;
use std::collections::BTreeMap;
use std::path::Path;

pub(crate) use round::{CampaignPlan, PublishedSelection};

/// Root moment plus one history event per control event and child moment.
const ROOT_HISTORY_EVENTS: u64 = 1;

/// Product state of one retained candidate.
#[derive(Clone)]
pub(super) struct CampaignEntry {
    /// Parent moment whose snapshot the candidate restores.
    pub(super) moment_id: Id,
    /// Product sequence number.
    pub(super) seq: u64,
    /// Legacy score.
    pub(super) score: f64,
    /// Selections so far (guidance generation).
    pub(super) times_selected: u32,
    /// Product depth.
    pub(super) depth: u32,
    /// Parent fault schedule.
    pub(super) schedule: FaultSchedule,
}

/// How a resumed checkpoint's journal binding compares with the recovered journal.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum CheckpointBinding {
    /// No checkpoint binding was offered.
    #[default]
    Absent,
    /// The bound segment exists with the bound digest.
    Matched,
    /// The bound segment is missing or differs; the journal wins.
    Mismatched,
}

/// What recovery found and closed when the journal was opened.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RecoverySummary {
    /// Epochs present before this run.
    pub(crate) recovered_epochs: usize,
    /// Outstanding selections closed unobserved.
    pub(crate) closed_selections: usize,
    /// Epoch closed with a `resume-epoch-closed` stop, if one was open.
    pub(crate) closed_epoch: Option<u32>,
    /// Comparison of the resumed checkpoint's journal binding.
    pub(crate) checkpoint_binding: CheckpointBinding,
}

fn compare_binding(
    recovered: &crate::explorer::journal::RecoveredJournal,
    binding: Option<&CampaignJournalBinding>,
) -> CheckpointBinding {
    let Some(binding) = binding else {
        return CheckpointBinding::Absent;
    };
    let Ok(digest) = binding.digest_bytes() else {
        return CheckpointBinding::Mismatched;
    };
    let matched = recovered
        .epochs()
        .iter()
        .find(|epoch| epoch.epoch() == binding.epoch)
        .and_then(|epoch| {
            epoch
                .segments()
                .iter()
                .find(|segment| segment.seq() == binding.segment_seq)
        })
        .is_some_and(|segment| *segment.digest().as_bytes() == digest);
    if matched {
        CheckpointBinding::Matched
    } else {
        CheckpointBinding::Mismatched
    }
}

/// Campaign-backed frontier.
pub(crate) struct CampaignFrontier<Io: PublicationIo = SystemPublicationIo> {
    identity: AdapterIdentity,
    config: ExplorerConfig,
    generation_profile_id: Id,
    journal: CampaignJournal<Io>,
    epoch: u32,
    previous_epoch_digest: Option<[u8; 32]>,
    recovery: RecoverySummary,
    profile: Option<CampaignProfile>,
    session: Option<ControlSession>,
    entries: BTreeMap<Id, CampaignEntry>,
    snapshots: SnapshotTable<SimulationSnapshot>,
    snapshot_context: Option<SnapshotContext>,
    next_seq: u64,
    last_segment: Option<CommittedSegment>,
    halted: bool,
    /// Test hook: a Choregraph projection bound below the derived one.
    #[cfg(test)]
    projection_bound: Option<u64>,
    /// Test hook: a Choregraph graph event limit below the derived one.
    #[cfg(test)]
    graph_bound: Option<u64>,
}

impl CampaignFrontier<SystemPublicationIo> {
    /// Admits the configuration and opens, recovers, and advances the journal
    /// under `output_dir`. Runs before any KVM work.
    pub(crate) fn open(
        config: &ExplorerConfig,
        output_dir: &Path,
        checkpoint: Option<&CampaignJournalBinding>,
    ) -> Result<Self, FrontierError> {
        Self::open_with_io(config, output_dir, checkpoint, SystemPublicationIo)
    }
}

impl<Io: PublicationIo> CampaignFrontier<Io> {
    /// [`CampaignFrontier::open`] with injected journal publication effects.
    pub(crate) fn open_with_io(
        config: &ExplorerConfig,
        output_dir: &Path,
        checkpoint: Option<&CampaignJournalBinding>,
        io: Io,
    ) -> Result<Self, FrontierError> {
        let identity = AdapterIdentity::for_seed(config.seed);
        // Degenerate configurations fail before the journal or KVM is touched.
        admit_config(
            &identity,
            config,
            BootstrapFacts {
                restorable_root: true,
            },
        )?;
        let generation_profile_id = generation_profile_id(config)?;
        let (journal, recovered) = CampaignJournal::open_with_io(output_dir, io)?;
        let mut frontier = Self {
            identity,
            config: config.clone(),
            generation_profile_id,
            journal,
            epoch: 0,
            previous_epoch_digest: None,
            recovery: RecoverySummary {
                recovered_epochs: recovered.epochs().len(),
                checkpoint_binding: compare_binding(&recovered, checkpoint),
                ..RecoverySummary::default()
            },
            profile: None,
            session: None,
            entries: BTreeMap::new(),
            snapshots: SnapshotTable::new(),
            snapshot_context: None,
            next_seq: 0,
            last_segment: None,
            halted: false,
            #[cfg(test)]
            projection_bound: None,
            #[cfg(test)]
            graph_bound: None,
        };
        if let Some(last) = recovered.epochs().last() {
            frontier.previous_epoch_digest = last.final_digest().map(|digest| *digest.as_bytes());
            if let Some(replayed) = replay_epoch(last)? {
                if let Some(session) = replayed.session {
                    frontier.close_recovered_epoch(&replayed.identity, session, last.epoch())?;
                }
            }
        }
        frontier.epoch = frontier.journal.begin_epoch()?;
        Ok(frontier)
    }

    /// Closes every outstanding selection of a recovered epoch unobserved and
    /// stops it, unless it already stopped.
    fn close_recovered_epoch(
        &mut self,
        identity: &AdapterIdentity,
        session: ControlSession,
        epoch: u32,
    ) -> Result<(), FrontierError> {
        if session.state().stopped.is_some() {
            return Ok(());
        }
        let mut staged = session;
        let mut records = Vec::new();
        let outstanding: Vec<Id> = staged
            .state()
            .outstanding
            .iter()
            .map(|selection| selection.selection_id)
            .collect();
        for selection_id in &outstanding {
            let decision = staged
                .payloads()
                .iter()
                .find_map(|payload| match &payload.control_event {
                    CampaignControlEvent::SelectionPublished(event)
                        if event.decision.selection_id == *selection_id =>
                    {
                        Some(event.decision.clone())
                    }
                    _ => None,
                })
                .ok_or(FrontierError::UnknownCandidate)?;
            let closure =
                unobserved_closure(identity, &decision, AdapterReason::SnapshotStateLost)?;
            let event = campaign_core::close_unobserved(staged.state(), closure)?;
            publish(&mut staged, &mut records, event)?;
        }
        let stop = campaign_core::admit_stop(
            staged.state(),
            StopReason::ConsumerRequested {
                reason_id: identity.reason_id(AdapterReason::ResumeEpochClosed),
            },
        )?;
        publish(&mut staged, &mut records, stop)?;
        staged.seal()?;
        let committed = self.append(&records)?;
        self.previous_epoch_digest = Some(*committed.digest().as_bytes());
        self.recovery.closed_selections = outstanding.len();
        self.recovery.closed_epoch = Some(epoch);
        Ok(())
    }

    /// What recovery found and closed.
    pub(crate) const fn recovery(&self) -> RecoverySummary {
        self.recovery
    }

    /// Journal epoch this run writes.
    pub(crate) const fn epoch(&self) -> u32 {
        self.epoch
    }

    /// Last committed segment (bound into `checkpoint.json`).
    pub(crate) const fn last_segment(&self) -> Option<CommittedSegment> {
        self.last_segment
    }

    fn session(&self) -> Result<&ControlSession, FrontierError> {
        self.session.as_ref().ok_or(FrontierError::Sequence(
            "campaign frontier used before genesis",
        ))
    }

    fn profile(&self) -> Result<&CampaignProfile, FrontierError> {
        self.profile.as_ref().ok_or(FrontierError::Sequence(
            "campaign frontier used before genesis",
        ))
    }

    fn snapshot_context(&self) -> Result<SnapshotContext, FrontierError> {
        self.snapshot_context.ok_or(FrontierError::Sequence(
            "campaign frontier used before genesis",
        ))
    }

    fn ensure_live(&self) -> Result<(), FrontierError> {
        if self.halted || self.journal.is_poisoned() {
            return Err(FrontierError::Halted);
        }
        Ok(())
    }

    /// Publishes one segment. Any failure but a sequence collision halts the
    /// backend; the caller drops its staged state either way.
    fn append(&mut self, records: &[JournalRecord]) -> Result<CommittedSegment, FrontierError> {
        match self.journal.append(records) {
            Ok(committed) => {
                self.last_segment = Some(committed);
                Ok(committed)
            }
            Err(error @ JournalError::DestinationExists { .. }) => Err(error.into()),
            Err(error) => {
                self.halted = true;
                Err(error.into())
            }
        }
    }

    /// Stages product entries and Campaign candidates plus guidance for proposals.
    fn stage_candidates(
        &self,
        staged: &mut ControlSession,
        records: &mut Vec<JournalRecord>,
        proposals: &[StagedProposal<'_>],
        next_seq: &mut u64,
    ) -> Result<Vec<(Id, CampaignEntry)>, FrontierError> {
        if proposals.is_empty() {
            return Ok(Vec::new());
        }
        let profile = self.profile()?;
        let mut candidates = Vec::with_capacity(proposals.len());
        let mut admitted = Vec::with_capacity(proposals.len());
        for proposal in proposals {
            let seq = *next_seq;
            *next_seq = next_seq
                .checked_add(1)
                .ok_or(FrontierError::BoundOverflow)?;
            let facts = FrontierEntryFacts {
                parent_moment_id: proposal.moment_id,
                mode: self.config.exploration_mode,
                entry_seq: seq,
                marker_digest: proposal.proposal.marker.as_ref().map(marker_digest),
            };
            let record = candidate_record(
                &self.identity,
                staged.state(),
                &facts,
                self.generation_profile_id,
            )?;
            admitted.push((
                record.candidate_id,
                CampaignEntry {
                    moment_id: proposal.moment_id,
                    seq,
                    score: proposal.proposal.score,
                    times_selected: 0,
                    depth: proposal.depth,
                    schedule: proposal.schedule.clone(),
                },
            ));
            records.push(encode(
                ENTRY,
                &EntryRecord {
                    candidate: record.candidate_id.into(),
                    moment: proposal.moment_id.into(),
                    seq,
                    score_bits: proposal.proposal.score.to_bits(),
                    depth: proposal.depth,
                    marker: proposal.proposal.marker.is_some(),
                },
            )?);
            candidates.push(record);
        }
        let batch = campaign_core::admit_candidates(
            staged.state(),
            candidates,
            profile.admission_rule,
            self.identity.coalescing_policy_id,
        )?;
        publish(staged, records, batch)?;
        let campaign_id = staged.state().admission.campaign_id;
        for (candidate_id, entry) in &admitted {
            let guidance = guidance_record(
                &self.identity,
                campaign_id,
                *candidate_id,
                GuidanceFacts {
                    score: entry.score,
                    times_selected: entry.times_selected,
                    entry_seq: entry.seq,
                },
            )?;
            let event = campaign_core::admit_guidance(staged.state(), guidance)?;
            publish(staged, records, event)?;
        }
        Ok(admitted)
    }

    /// Adopts a committed round's product entries.
    fn adopt_entries(&mut self, admitted: Vec<(Id, CampaignEntry)>, next_seq: u64) {
        self.entries.extend(admitted);
        self.next_seq = next_seq;
    }

    /// Drops product entries Campaign no longer retains and releases snapshots
    /// no retained candidate references; moments stay in history.
    fn release_unretained(&mut self) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let retained: BTreeMap<Id, ()> = session
            .state()
            .candidates
            .iter()
            .filter(|candidate| candidate.status == CandidateStatus::Retained)
            .map(|candidate| (candidate.candidate.candidate_id, ()))
            .collect();
        self.entries
            .retain(|candidate_id, _| retained.contains_key(candidate_id));
        let referenced: BTreeMap<Id, ()> = self
            .entries
            .values()
            .map(|entry| (entry.moment_id, ()))
            .collect();
        let moments: Vec<Id> = self.snapshot_moments();
        for moment_id in moments {
            if !referenced.contains_key(&moment_id) {
                self.snapshots.release(moment_id);
            }
        }
    }

    fn snapshot_moments(&self) -> Vec<Id> {
        self.snapshots.moments().collect()
    }

    /// Admits the stop of the run; `Completed` without any admitted selection is
    /// recorded as `completed-without-selection`.
    fn stop_reason_for(&self, stop: ProductStop) -> Result<StopReason, FrontierError> {
        let profile = self.profile()?;
        let stop = match (stop, profile.selection_limit) {
            (ProductStop::Completed, SelectionLimit::NoSelection) => {
                ProductStop::CompletedWithoutSelection
            }
            (stop, _) => stop,
        };
        Ok(stop_reason(&self.identity, stop))
    }

    /// Choregraph projection bound of the epoch: the whole history bound, which
    /// holds every control event the profile budgets.
    #[cfg(not(test))]
    #[allow(clippy::unused_self)]
    const fn projection_bound(&self, max_history_events: u64) -> u64 {
        max_history_events
    }

    /// Choregraph projection bound of the epoch, tightened by the test hook.
    #[cfg(test)]
    fn projection_bound(&self, max_history_events: u64) -> u64 {
        self.projection_bound
            .map_or(max_history_events, |bound| bound.min(max_history_events))
    }

    /// Choregraph graph event limit of the epoch: the whole history bound.
    #[cfg(not(test))]
    #[allow(clippy::unused_self)]
    const fn graph_bound(&self, max_history_events: u64) -> u64 {
        max_history_events
    }

    /// Choregraph graph event limit of the epoch, tightened by the test hook.
    #[cfg(test)]
    fn graph_bound(&self, max_history_events: u64) -> u64 {
        self.graph_bound
            .map_or(max_history_events, |bound| bound.min(max_history_events))
    }

    /// Tightens the Choregraph projection bound of the next genesis (test hook
    /// for the host control capacity stop).
    #[cfg(test)]
    pub(super) fn tighten_projection_bound(&mut self, bound: u64) {
        self.projection_bound = Some(bound);
    }

    /// Tightens the Choregraph graph event limit of the next genesis (test hook
    /// for the host control capacity stop at the graph bound).
    #[cfg(test)]
    pub(super) fn tighten_graph_bound(&mut self, bound: u64) {
        self.graph_bound = Some(bound);
    }

    #[cfg(test)]
    pub(super) fn journal(&self) -> &CampaignJournal<Io> {
        &self.journal
    }

    #[cfg(test)]
    pub(super) fn journal_mut(&mut self) -> &mut CampaignJournal<Io> {
        &mut self.journal
    }

    /// Drops the retained snapshot of every candidate's parent (test hook for
    /// ineligibility).
    #[cfg(test)]
    pub(super) fn forget_snapshots(&mut self) {
        for moment_id in self.snapshot_moments() {
            self.snapshots.release(moment_id);
        }
    }

    #[cfg(test)]
    pub(super) fn state(&self) -> Option<&campaign_core::FrontierState> {
        self.session.as_ref().map(ControlSession::state)
    }
}

/// One proposal staged as a candidate below a moment.
pub(super) struct StagedProposal<'a> {
    pub(super) moment_id: Id,
    pub(super) depth: u32,
    pub(super) schedule: &'a FaultSchedule,
    pub(super) proposal: &'a super::EntryProposal,
}

/// Plans one control event on the staged session and records it.
pub(super) fn publish(
    staged: &mut ControlSession,
    records: &mut Vec<JournalRecord>,
    event: CampaignControlEvent,
) -> Result<campaign_choregraph::ControlPublication, FrontierError> {
    let bytes = encode_control_event(&event).map_err(AdapterError::Wire)?;
    let publication = staged.publish(event)?;
    records.push(JournalRecord::new(CONTROL, bytes));
    Ok(publication)
}

impl<Io: PublicationIo> FrontierBackend for CampaignFrontier<Io> {
    fn admit_root(&mut self, root: RootAdmission) -> Result<(), FrontierError> {
        self.ensure_live()?;
        if self.session.is_some() {
            return Err(FrontierError::Sequence("campaign root admitted twice"));
        }
        let restorable_root = root.snapshot.is_some();
        let profile = admit_config(
            &self.identity,
            &self.config,
            BootstrapFacts { restorable_root },
        )?;
        let Some(snapshot) = root.snapshot else {
            return Err(FrontierError::Sequence("restorable root checked above"));
        };
        let bounds = &profile.bounds;
        let moments = bounds
            .max_selections
            .checked_mul(bounds.max_child_moments)
            .ok_or(FrontierError::BoundOverflow)?;
        let max_history_events = ROOT_HISTORY_EVENTS
            .checked_add(bounds.max_control_events)
            .and_then(|events| events.checked_add(moments))
            .ok_or(FrontierError::BoundOverflow)?;
        let limits = history_limits(
            self.graph_bound(max_history_events),
            self.projection_bound(max_history_events),
        )?;
        let epoch = u64::from(self.epoch);
        let digests = root
            .digests
            .ok_or(FrontierError::Sequence("root admission without digests"))?;
        let facts = RootMomentFacts {
            epoch,
            config_digest: config_digest(&self.config, self.generation_profile_id)?,
            seed: self.config.seed,
            bootstrap_tick: digests.total_ticks,
            coverage_digest: digests.coverage,
            oracle_digest: digests.oracle,
            base_set_id: root.base_set_id,
        };
        let empty = HistoryGraph::new(limits).map_err(AdapterError::from)?;
        let moment = admit_root_moment(&empty, &self.identity, &facts)?;
        let binding =
            projection_binding(&self.identity, &moment.graph, &root_control_branch(&moment))?;
        let admission = campaign_core::admit_campaign(campaign_admission(
            &self.identity,
            &profile,
            &moment,
            binding,
        ))?;
        let mut records = vec![
            encode(
                GENESIS,
                &GenesisRecord {
                    schema: GENESIS_SCHEMA.to_owned(),
                    epoch: self.epoch,
                    seed: self.config.seed,
                    max_history_events,
                    previous_epoch_digest: self.previous_epoch_digest,
                },
            )?,
            JournalRecord::new(
                MOMENT,
                encode_moment(moment.event, &root_moment_draft(&self.identity, &facts))
                    .map_err(AdapterError::Wire)?,
            ),
            JournalRecord::new(
                CONTROL,
                encode_control_event(&admission).map_err(AdapterError::Wire)?,
            ),
        ];
        let mut staged =
            ControlSession::genesis(self.identity.clone(), limits, &moment, admission)?;
        self.profile = Some(profile);
        let proposals: Vec<StagedProposal<'_>> = root
            .proposals
            .iter()
            .map(|proposal| StagedProposal {
                moment_id: moment.moment_id,
                depth: 0,
                schedule: &root.schedule,
                proposal,
            })
            .collect();
        let mut next_seq = self.next_seq;
        let admitted =
            self.stage_candidates(&mut staged, &mut records, &proposals, &mut next_seq)?;
        // Legacy pushes root proposals through its bounded frontier, so more
        // bootstrap markers than `max_frontier` are pruned at genesis too.
        // r[impl chaoscontrol.campaign_policy.frontier_parity]
        self.stage_prune(&mut staged, &mut records)?;
        self.commit(staged, &records)?;
        let context = SnapshotContext {
            base_set_id: root.base_set_id,
            epoch,
        };
        self.snapshots.retain(
            moment.moment_id,
            SnapshotEntry {
                handle: snapshot,
                base_set_id: root.base_set_id,
                epoch,
            },
        )?;
        self.snapshot_context = Some(context);
        self.adopt_entries(admitted, next_seq);
        self.release_unretained();
        Ok(())
    }

    fn begin_round(
        &mut self,
        rng: &mut dyn RngCore,
        context: RoundContext,
    ) -> Result<RoundBegin, FrontierError> {
        self.begin(rng, context)
    }

    fn authorize(
        &mut self,
        planned: super::PlannedRound,
    ) -> Result<super::Authorization, FrontierError> {
        self.authorize_planned(planned)
    }

    fn finish_round(
        &mut self,
        round: super::AuthorizedRound,
        observation: &super::RoundObservation,
    ) -> Result<super::Settlement, FrontierError> {
        self.settle(round, observation)
    }

    fn record_stop(&mut self, stop: ProductStop) -> Result<(), FrontierError> {
        self.ensure_live()?;
        let session = self.session()?;
        if session.state().stopped.is_some() {
            return Ok(());
        }
        let reason = self.stop_reason_for(stop)?;
        let mut staged = session.clone();
        let mut records = Vec::new();
        let event = campaign_core::admit_stop(staged.state(), reason)?;
        publish(&mut staged, &mut records, event)?;
        self.commit(staged, &records)
    }

    fn retained(&self) -> Vec<RetainedEntry> {
        let mut retained: Vec<RetainedEntry> = self
            .entries
            .values()
            .map(|entry| RetainedEntry {
                seq: entry.seq,
                times_selected: entry.times_selected,
                score_bits: entry.score.to_bits(),
            })
            .collect();
        retained.sort_by_key(|entry| entry.seq);
        retained
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    /// Campaign's own counters: published selections and reserved budget.
    fn selection_use(&self) -> SelectionUse {
        let Some(session) = self.session.as_ref() else {
            return SelectionUse::default();
        };
        let state = session.state();
        SelectionUse {
            selections: state.selection_count,
            budget_units: state
                .reserved_resources
                .iter()
                .filter(|reserved| reserved.dimension_id == self.identity.resource_dimension_id)
                .map(|reserved| reserved.amount)
                .fold(0, u64::saturating_add),
        }
    }
}
