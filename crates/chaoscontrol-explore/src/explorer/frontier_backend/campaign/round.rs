//! Campaign round steps: begin (draw + plan), authorize (selection segment +
//! execution gate + eligibility), and settle (settlement segment).

use super::{publish, CampaignEntry, CampaignFrontier, StagedProposal};
use crate::campaign_adapter::{
    admit_child_moment, child_moment_draft, decay_revision, encode_moment, execution_authority,
    finding_ref, ineligible_retirement, is_capacity_refusal, result_observation,
    retention_plan_request, round_decision, round_fits_capacity, selection_request,
    ticket_explores, unobserved_closure, AdapterError, ChildMomentFacts, Eligibility, EntropyDraws,
    GuidanceFacts, HostCapacity, IneligibleReason, LegacyDraws, ProductStop, ResultFacts,
    RetainedChild, RoundDecision, SelectionLimit,
};
use crate::explorer::frontier_backend::digests::round_manifest_digest;
use crate::explorer::frontier_backend::records::{
    encode, RoundRecord, SelectionRecord, MOMENT, ROUND, SELECTION,
};
use crate::explorer::frontier_backend::session::ControlSession;
use crate::explorer::frontier_backend::{
    Authorization, AuthorizedDetail, AuthorizedRound, FrontierError, PlanDetail, PlannedRound,
    RoundBegin, RoundContext, RoundDraws, RoundObservation, SelectedEntry, SelectionWork,
    Settlement, SkippedRound,
};
use crate::explorer::journal::{JournalError, JournalRecord, PublicationIo};
use crate::frontier::draw_selection;
use campaign_choregraph::{
    plan_next_from_published_projection, DurablePublicationEvidence, PublishedSelectionRequest,
};
use campaign_core::{
    CampaignControlEvent, CandidateStatus, Id, SelectionDecision, SelectionEvent, SelectionRequest,
};
use rand::RngCore;

/// Replans after a selection-segment sequence collision.
const MAX_STALE_REPLANS: u32 = 1;

/// A planned Campaign selection and the draws it was planned from.
pub(crate) struct CampaignPlan {
    draws: LegacyDraws,
    request: SelectionRequest,
    selection: SelectionDecision,
    /// Whether the planned request's entropy ticket takes Campaign's explore branch.
    explored: bool,
}

/// A durably published and authorized selection.
pub(crate) struct PublishedSelection {
    selection: SelectionDecision,
}

impl<Io: PublicationIo> CampaignFrontier<Io> {
    /// Retires ineligible candidates, checks the host control capacity, draws in
    /// legacy order, and plans.
    pub(super) fn begin(
        &mut self,
        rng: &mut dyn RngCore,
        context: RoundContext,
    ) -> Result<RoundBegin, FrontierError> {
        self.ensure_live()?;
        if self.session()?.state().stopped.is_some() {
            return Err(FrontierError::Sequence("round after the campaign stopped"));
        }
        // `max_rounds == 0` admits no selection: stop before drawing or planning.
        if self.profile()?.selection_limit == SelectionLimit::NoSelection {
            return Ok(RoundBegin::Stop(ProductStop::CompletedWithoutSelection));
        }
        // A retirement the host capacity (projection, generation, or graph
        // bound) refuses was not staged; Campaign keeps the stop reserve, so the
        // capacity stop still publishes.
        match self.retire_ineligible() {
            Err(FrontierError::Adapter(error)) if is_capacity_refusal(&error) => {
                return Ok(RoundBegin::Stop(ProductStop::ControlCapacityExhausted));
            }
            result => result?,
        }
        let retained = self.retained_candidates()?;
        if retained != self.entries.len() {
            return Err(FrontierError::StateDivergence);
        }
        // r[impl chaoscontrol.campaign_policy.stop]
        if retained > 0 && !self.round_fits_capacity()? {
            return Ok(RoundBegin::Stop(ProductStop::ControlCapacityExhausted));
        }
        let Some(draws) = draw_selection(rng, retained) else {
            return Ok(RoundBegin::Stop(ProductStop::FrontierExhausted));
        };
        self.plan(context, draws)
    }

    /// Whether the held projection's host capacity still holds a whole round
    /// (child moments included) beside Campaign's exposed reserve: the round's
    /// graph appends stay within the projection's `graph_room`.
    fn round_fits_capacity(&self) -> Result<bool, FrontierError> {
        let capacity = HostCapacity::of(self.session()?.projection())?;
        Ok(round_fits_capacity(
            &capacity,
            self.profile()?.round_demand()?,
        )?)
    }

    fn retained_candidates(&self) -> Result<usize, FrontierError> {
        Ok(self
            .session()?
            .state()
            .candidates
            .iter()
            .filter(|candidate| candidate.status == CandidateStatus::Retained)
            .count())
    }

    /// Plans one selection from recorded draws; never draws.
    fn plan(&self, context: RoundContext, draws: LegacyDraws) -> Result<RoundBegin, FrontierError> {
        let session = self.session()?;
        let profile = self.profile()?;
        let entropy = EntropyDraws::from_legacy(draws, self.entries.len())?;
        let binding = session.binding()?;
        let request =
            selection_request(&self.identity, profile, session.state(), &binding, entropy)?;
        // Planning through the held projection keeps Campaign's own host
        // capacity reserve: without room for one more selection, its settlement,
        // and the stop, the plan is the capacity stop (or a result wait). Campaign
        // checks the recorded reserve on every plan (D83), so a typed capacity
        // error means the reserve itself no longer fits: something outside
        // Campaign publication consumed it, the stop would be refused too, so it
        // is not mapped to a stop and propagates. The whole-round check above
        // keeps this shell's own appends within Campaign's exposed graph room,
        // and the shell never leaves a control draft, so it is not expected.
        let plan = plan_next_from_published_projection(
            session.projection(),
            PublishedSelectionRequest {
                request: request.clone(),
            },
        )?;
        let work = SelectionWork {
            candidates_examined: plan.work.candidates_examined,
            eligible_candidates: plan.work.eligible_candidates,
        };
        match round_decision(&self.identity, plan, session.state().admission.campaign_id)? {
            RoundDecision::Execute(selection) => {
                if !self.entries.contains_key(&selection.candidate_id) {
                    return Err(FrontierError::UnknownCandidate);
                }
                let explored = ticket_explores(&request.profile.mode, request.entropy.as_ref())?;
                Ok(RoundBegin::Planned(PlannedRound {
                    context,
                    draws: RoundDraws { draws: Some(draws) },
                    work,
                    plan: PlanDetail::Campaign(Box::new(CampaignPlan {
                        draws,
                        request,
                        selection: *selection,
                        explored,
                    })),
                }))
            }
            // r[impl chaoscontrol.campaign_policy.stop.wait]
            RoundDecision::Wait(reason) => Err(FrontierError::UnexpectedWait(reason)),
            RoundDecision::Stop(stop) => Ok(RoundBegin::Stop(stop)),
        }
    }

    /// Publishes the selection segment, then gates execution on Campaign's
    /// durable-publication check and the parent snapshot's eligibility.
    // r[impl chaoscontrol.campaign_policy.publication_fence]
    pub(super) fn authorize_planned(
        &mut self,
        planned: PlannedRound,
    ) -> Result<Authorization, FrontierError> {
        self.ensure_live()?;
        let PlanDetail::Campaign(plan) = planned.plan else {
            return Err(FrontierError::Sequence("legacy plan offered to campaign"));
        };
        let context = planned.context;
        let work = planned.work;
        let mut plan = *plan;
        let mut replans = 0;
        let publication = loop {
            let (staged, records, publication) = self.stage_selection(context, &plan)?;
            match self.append(&records) {
                Ok(_) => {
                    self.session = Some(staged);
                    break publication;
                }
                Err(FrontierError::Journal(JournalError::DestinationExists { .. }))
                    if replans < MAX_STALE_REPLANS =>
                {
                    replans += 1;
                    plan = self.replan_after_collision(context, plan.draws)?;
                }
                Err(FrontierError::Journal(JournalError::DestinationExists { .. })) => {
                    self.halted = true;
                    return Err(FrontierError::StaleControlBranch);
                }
                Err(error) => return Err(error),
            }
        };
        let evidence = DurablePublicationEvidence {
            history_event_id: publication.history_event_id(),
            branch_before: publication.branch_plan().before.clone(),
            branch_after: publication.branch_plan().after.clone(),
            durable_event: true,
            durable_branch_update: true,
        };
        if !execution_authority(&plan.selection, Some(&publication), Some(&evidence)) {
            self.halted = true;
            return Err(FrontierError::ExecutionRefused);
        }
        self.executable(context, work, plan)
    }

    fn stage_selection(
        &self,
        context: RoundContext,
        plan: &CampaignPlan,
    ) -> Result<
        (
            ControlSession,
            Vec<JournalRecord>,
            campaign_choregraph::ControlPublication,
        ),
        FrontierError,
    > {
        let entry = self
            .entries
            .get(&plan.selection.candidate_id)
            .ok_or(FrontierError::UnknownCandidate)?;
        let index = plan
            .draws
            .index
            .map(u64::try_from)
            .transpose()
            .map_err(|_| FrontierError::BoundOverflow)?;
        let mut records = vec![encode(
            SELECTION,
            &SelectionRecord {
                round: context.round,
                mode: context.mode.as_str().to_owned(),
                unit_bits: plan.draws.unit.to_bits(),
                index,
                candidate: plan.selection.candidate_id.into(),
                selection: plan.selection.selection_id.into(),
                seq: entry.seq,
            },
        )?];
        let mut staged = self.session()?.clone();
        let event = SelectionEvent::new(
            plan.selection.clone(),
            plan.request.profile.clone(),
            plan.request.entropy.clone(),
        )?;
        let publication = publish(
            &mut staged,
            &mut records,
            CampaignControlEvent::SelectionPublished(event),
        )?;
        staged.seal()?;
        Ok((staged, records, publication))
    }

    /// Resynchronizes after a sequence collision and replans once with the same
    /// draws. The rebuilt history must be exactly this writer's state.
    fn replan_after_collision(
        &mut self,
        context: RoundContext,
        draws: LegacyDraws,
    ) -> Result<CampaignPlan, FrontierError> {
        let recovered = self.journal.resynchronize()?;
        let current = self.session()?.state().state_id;
        let rebuilt = recovered
            .epochs()
            .last()
            .map(super::super::records::replay_epoch)
            .transpose()?
            .flatten()
            .and_then(|epoch| epoch.session)
            .map(|session| session.state().state_id);
        if rebuilt != Some(current) {
            self.halted = true;
            return Err(FrontierError::StaleControlBranch);
        }
        match self.plan(context, draws)? {
            RoundBegin::Planned(PlannedRound {
                plan: PlanDetail::Campaign(plan),
                ..
            }) => Ok(*plan),
            _ => {
                self.halted = true;
                Err(FrontierError::StaleControlBranch)
            }
        }
    }

    /// Checks the published selection's parent snapshot after publication.
    // r[impl chaoscontrol.campaign_policy.snapshot_eligibility]
    fn executable(
        &mut self,
        context: RoundContext,
        work: SelectionWork,
        plan: CampaignPlan,
    ) -> Result<Authorization, FrontierError> {
        let candidate_id = plan.selection.candidate_id;
        let entry = self
            .entries
            .get(&candidate_id)
            .ok_or(FrontierError::UnknownCandidate)?;
        let record = self
            .session()?
            .state()
            .candidate(candidate_id)?
            .candidate
            .clone();
        let draws = RoundDraws {
            draws: Some(plan.draws),
        };
        match self
            .snapshots
            .require_executable(&record, self.snapshot_context()?)
        {
            Ok(snapshot) => {
                let selected = SelectedEntry {
                    seq: entry.seq,
                    snapshot: snapshot.handle.clone(),
                    schedule: entry.schedule.clone(),
                    depth: entry.depth,
                    explored: plan.explored,
                };
                Ok(Authorization::Execute(Box::new(AuthorizedRound::new(
                    context,
                    draws,
                    work,
                    Some(selected),
                    AuthorizedDetail::Campaign(Box::new(PublishedSelection {
                        selection: plan.selection,
                    })),
                ))))
            }
            Err(AdapterError::Ineligible(reason)) => {
                let seq = entry.seq;
                self.close_ineligible(&plan.selection, reason)?;
                Ok(Authorization::Skipped(SkippedRound {
                    context,
                    draws,
                    work,
                    seq,
                    explored: plan.explored,
                }))
            }
            Err(error) => Err(error.into()),
        }
    }

    /// Closes a published selection unobserved and retires its candidate.
    fn close_ineligible(
        &mut self,
        selection: &SelectionDecision,
        reason: IneligibleReason,
    ) -> Result<(), FrontierError> {
        let mut staged = self.session()?.clone();
        let mut records = Vec::new();
        let closure = unobserved_closure(&self.identity, selection, reason.reason())?;
        let event = campaign_core::close_unobserved(staged.state(), closure)?;
        publish(&mut staged, &mut records, event)?;
        let retirement = ineligible_retirement(
            &self.identity,
            staged.state(),
            selection.candidate_id,
            reason,
        );
        let event = campaign_core::retire_candidate(staged.state(), retirement)?;
        publish(&mut staged, &mut records, event)?;
        self.commit(staged, &records)?;
        self.release_unretained();
        Ok(())
    }

    /// Retires every retained candidate whose parent snapshot is not restorable.
    fn retire_ineligible(&mut self) -> Result<(), FrontierError> {
        let context = self.snapshot_context()?;
        let ineligible: Vec<(Id, IneligibleReason)> = self
            .entries
            .iter()
            .filter_map(|(candidate_id, entry)| {
                match self.snapshots.eligibility(entry.moment_id, context) {
                    Eligibility::Eligible => None,
                    Eligibility::Ineligible(reason) => Some((*candidate_id, reason)),
                }
            })
            .collect();
        if ineligible.is_empty() {
            return Ok(());
        }
        let mut staged = self.session()?.clone();
        let mut records = Vec::new();
        for (candidate_id, reason) in ineligible {
            let retirement =
                ineligible_retirement(&self.identity, staged.state(), candidate_id, reason);
            let event = campaign_core::retire_candidate(staged.state(), retirement)?;
            publish(&mut staged, &mut records, event)?;
        }
        self.commit(staged, &records)?;
        self.release_unretained();
        Ok(())
    }

    /// Seals and publishes a staged segment; any failure halts the backend.
    pub(super) fn commit(
        &mut self,
        mut staged: ControlSession,
        records: &[JournalRecord],
    ) -> Result<(), FrontierError> {
        staged.seal()?;
        if let Err(error) = self.append(records) {
            self.halted = true;
            return Err(error);
        }
        self.session = Some(staged);
        Ok(())
    }

    /// Settles an executed round: child moments, result, candidates, guidance,
    /// decay revision, and pruning in one settlement segment.
    // r[impl chaoscontrol.campaign_policy.observations]
    pub(super) fn settle(
        &mut self,
        round: AuthorizedRound,
        observation: &RoundObservation,
    ) -> Result<Settlement, FrontierError> {
        self.ensure_live()?;
        let context = round.context();
        let AuthorizedDetail::Campaign(published) = round.detail else {
            return Err(FrontierError::Sequence("legacy round settled by campaign"));
        };
        let selection = published.selection;
        let base_set_id = self.snapshot_context()?.base_set_id;
        let mut staged = self.session()?.clone();
        let campaign_id = staged.state().admission.campaign_id;
        let parent_event = staged
            .moment_event(selection.parent_moment_id)
            .ok_or(FrontierError::UnknownCandidate)?;
        let mut records = Vec::new();
        let mut children = Vec::new();
        let mut proposals = Vec::new();
        let mut snapshots = Vec::new();
        let mut findings = Vec::new();
        for branch in &observation.branches {
            findings.extend(branch.findings.iter().map(|finding| {
                finding_ref(
                    finding.dedup_key,
                    finding.assertion_fingerprint,
                    selection.parent_moment_id,
                )
            }));
            let Some(retained) = &branch.retained else {
                continue;
            };
            let digests = retained
                .digests
                .ok_or(FrontierError::Sequence("retained branch without digests"))?;
            let facts = ChildMomentFacts {
                campaign_id,
                selection_id: selection.selection_id,
                operation_id: selection.operation_id,
                branch_slot: branch.slot,
                schedule_digest: digests.schedule,
                variant_digest: digests.variant,
                coverage_digest: digests.coverage,
                oracle_digest: digests.oracle,
                total_ticks: digests.total_ticks,
                snapshot_tick: digests.total_ticks,
                base_set_id,
            };
            let moment = admit_child_moment(
                staged.graph(),
                &self.identity,
                campaign_id,
                &facts,
                parent_event,
            )?;
            let draft = child_moment_draft(&self.identity, &facts, parent_event)?;
            records.push(JournalRecord::new(
                MOMENT,
                encode_moment(moment.event, &draft).map_err(AdapterError::Wire)?,
            ));
            children.push(RetainedChild::from(&moment));
            snapshots.push((moment.moment_id, retained.snapshot.clone()));
            proposals.extend(retained.proposals.iter().map(|proposal| StagedProposal {
                moment_id: moment.moment_id,
                depth: retained.depth,
                schedule: &retained.schedule,
                proposal,
            }));
            staged.admit_moment(moment);
        }
        let result = result_observation(
            &self.identity,
            staged.state(),
            &selection,
            &ResultFacts {
                children,
                findings,
                round_manifest_digest: round_manifest_digest(observation)?,
            },
        )?;
        let event = campaign_core::admit_result(staged.state(), result)?;
        publish(&mut staged, &mut records, event)?;
        let mut next_seq = self.next_seq;
        let admitted =
            self.stage_candidates(&mut staged, &mut records, &proposals, &mut next_seq)?;
        let times_selected = self.stage_decay(&mut staged, &mut records, &selection)?;
        let pruned = self.stage_prune(&mut staged, &mut records)?;
        let admitted_seqs: Vec<u64> = admitted.iter().map(|(_, entry)| entry.seq).collect();
        let mut pruned_seqs: Vec<u64> = pruned
            .iter()
            .filter_map(|candidate_id| {
                self.entries
                    .get(candidate_id)
                    .or_else(|| {
                        admitted
                            .iter()
                            .find(|(admitted_id, _)| admitted_id == candidate_id)
                            .map(|(_, entry)| entry)
                    })
                    .map(|entry| entry.seq)
            })
            .collect();
        pruned_seqs.sort_unstable();
        records.push(encode(
            ROUND,
            &RoundRecord {
                round: context.round,
                selection: selection.selection_id.into(),
                manifest: round_manifest_digest(observation)?,
                admitted: admitted_seqs.clone(),
                pruned: pruned_seqs.clone(),
            },
        )?);
        self.commit(staged, &records)?;
        let epoch = u64::from(self.epoch);
        for (moment_id, snapshot) in snapshots {
            self.snapshots.retain(
                moment_id,
                crate::campaign_adapter::SnapshotEntry {
                    handle: snapshot,
                    base_set_id,
                    epoch,
                },
            )?;
        }
        if let Some(entry) = self.entries.get_mut(&selection.candidate_id) {
            entry.times_selected = times_selected;
        }
        self.adopt_entries(admitted, next_seq);
        self.release_unretained();
        Ok(Settlement {
            admitted: admitted_seqs,
            pruned: pruned_seqs,
        })
    }

    /// Stages the explicit division-decay revision of the selected candidate.
    fn stage_decay(
        &self,
        staged: &mut ControlSession,
        records: &mut Vec<JournalRecord>,
        selection: &SelectionDecision,
    ) -> Result<u32, FrontierError> {
        let entry: &CampaignEntry = self
            .entries
            .get(&selection.candidate_id)
            .ok_or(FrontierError::UnknownCandidate)?;
        let times_selected = entry
            .times_selected
            .checked_add(1)
            .ok_or(FrontierError::BoundOverflow)?;
        let latest = staged
            .state()
            .candidate(selection.candidate_id)?
            .latest_guidance
            .clone()
            .ok_or(FrontierError::Sequence(
                "selected candidate has no guidance",
            ))?;
        let request = decay_revision(
            &self.identity,
            &latest,
            GuidanceFacts {
                score: entry.score,
                times_selected,
                entry_seq: entry.seq,
            },
        )?;
        let event = campaign_core::revise_guidance(staged.state(), request)?;
        publish(staged, records, event)?;
        Ok(times_selected)
    }

    /// Stages one retention prune to `max_frontier` (rank order `[1, 2]`).
    pub(super) fn stage_prune(
        &self,
        staged: &mut ControlSession,
        records: &mut Vec<JournalRecord>,
    ) -> Result<Vec<Id>, FrontierError> {
        let request = retention_plan_request(&self.identity, staged.state());
        let Some(plan) = campaign_core::plan_pruning(staged.state(), request)? else {
            return Ok(Vec::new());
        };
        let pruned = plan.candidates.clone();
        let event = campaign_core::prune_frontier(staged.state(), plan)?;
        publish(staged, records, event)?;
        Ok(pruned)
    }
}
