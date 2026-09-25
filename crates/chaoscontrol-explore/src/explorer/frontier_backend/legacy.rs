//! Legacy in-process frontier behind the round protocol.
//!
//! Behavior is exactly the pre-policy frontier: the same draws through
//! [`draw_selection`], the same exploit/explore choice, `times_selected` counted at
//! selection, and every proposal pushed (with its per-push pruning) in branch
//! order. Pushing at settlement instead of inside the branch loop is equivalent
//! because nothing reads the frontier between a round's branches; branch scores
//! and marker hit counts are computed at branch time by the shell.

use super::{
    Authorization, AuthorizedDetail, AuthorizedRound, FrontierBackend, FrontierError, PlanDetail,
    PlannedRound, RetainedEntry, RootAdmission, RoundBegin, RoundContext, RoundDraws,
    RoundObservation, SelectedEntry, Settlement,
};
use crate::campaign_adapter::{legacy_explores, ProductStop};
use crate::coverage::CoverageBitmap;
use crate::frontier::{draw_selection, Frontier, FrontierEntry};
use chaoscontrol_fault::schedule::FaultSchedule;
use chaoscontrol_vmm::controller::SimulationSnapshot;
use rand_chacha::ChaCha8Rng;
use std::collections::BTreeSet;

/// Shared product state of the proposals of one retained branch.
struct PushSource<'a> {
    snapshot: &'a SimulationSnapshot,
    coverage: &'a CoverageBitmap,
    schedule: &'a FaultSchedule,
    depth: u32,
    parent: Option<u64>,
}

/// Legacy frontier backend.
pub(crate) struct LegacyFrontier {
    frontier: Frontier,
    /// Sequence number the next push receives (mirrors the frontier's id counter).
    next_seq: u64,
}

impl LegacyFrontier {
    /// Empty frontier bounded by `max_frontier`.
    pub(crate) fn new(max_frontier: usize) -> Self {
        Self {
            frontier: Frontier::new(max_frontier),
            next_seq: 0,
        }
    }

    /// Pushes one recycled corpus entry (legacy-only empty-frontier recovery).
    pub(crate) fn push_recycled(&mut self, entry: FrontierEntry) {
        self.push(entry);
    }

    fn push(&mut self, entry: FrontierEntry) -> u64 {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        self.frontier.push(entry);
        seq
    }

    fn ids(&self) -> BTreeSet<u64> {
        self.frontier
            .entries()
            .iter()
            .map(|entry| entry.id)
            .collect()
    }

    fn push_proposals(
        &mut self,
        source: &PushSource<'_>,
        proposals: &[super::EntryProposal],
        admitted: &mut Vec<u64>,
    ) {
        for proposal in proposals {
            admitted.push(self.push(FrontierEntry {
                id: 0,
                snapshot: source.snapshot.clone(),
                coverage: source.coverage.clone(),
                score: proposal.score,
                times_selected: 0,
                depth: source.depth,
                schedule: source.schedule.clone(),
                parent: source.parent,
                marker: proposal.marker.clone(),
            }));
        }
    }
}

impl FrontierBackend for LegacyFrontier {
    fn admit_root(&mut self, root: RootAdmission) -> Result<(), FrontierError> {
        if let Some(snapshot) = root.snapshot {
            let mut admitted = Vec::with_capacity(root.proposals.len());
            let source = PushSource {
                snapshot: &snapshot,
                coverage: &root.coverage,
                schedule: &root.schedule,
                depth: 0,
                parent: None,
            };
            self.push_proposals(&source, &root.proposals, &mut admitted);
        }
        Ok(())
    }

    fn begin_round(
        &mut self,
        rng: &mut ChaCha8Rng,
        context: RoundContext,
    ) -> Result<RoundBegin, FrontierError> {
        let draws = draw_selection(rng, self.frontier.len());
        let index = match draws {
            None => None,
            Some(draws) => Some(
                self.frontier
                    .selection_index(draws)
                    .ok_or(FrontierError::Sequence("legacy draw selects no entry"))?,
            ),
        };
        Ok(RoundBegin::Planned(PlannedRound {
            context,
            draws: RoundDraws { draws },
            plan: PlanDetail::Legacy { index },
        }))
    }

    fn authorize(&mut self, planned: PlannedRound) -> Result<Authorization, FrontierError> {
        let PlanDetail::Legacy { index } = planned.plan else {
            return Err(FrontierError::Sequence("campaign plan offered to legacy"));
        };
        let explored = planned
            .draws
            .draws
            .is_some_and(|draws| legacy_explores(draws.unit));
        let selected = match index {
            None => None,
            Some(index) => {
                let entry = self
                    .frontier
                    .select_index(index)
                    .ok_or(FrontierError::UnknownCandidate)?;
                Some(SelectedEntry {
                    seq: entry.id,
                    snapshot: entry.snapshot.clone(),
                    schedule: entry.schedule.clone(),
                    depth: entry.depth,
                    explored,
                })
            }
        };
        Ok(Authorization::Execute(Box::new(AuthorizedRound::new(
            planned.context,
            planned.draws,
            selected,
            AuthorizedDetail::Legacy,
        ))))
    }

    fn finish_round(
        &mut self,
        round: AuthorizedRound,
        observation: &RoundObservation,
    ) -> Result<Settlement, FrontierError> {
        let parent = round.selected().map(|selected| selected.seq);
        let before = self.ids();
        let mut admitted = Vec::new();
        for branch in &observation.branches {
            if let Some(retained) = &branch.retained {
                let source = PushSource {
                    snapshot: &retained.snapshot,
                    coverage: &retained.coverage,
                    schedule: &retained.schedule,
                    depth: retained.depth,
                    parent,
                };
                self.push_proposals(&source, &retained.proposals, &mut admitted);
            }
        }
        let after = self.ids();
        let mut pruned: Vec<u64> = before
            .iter()
            .chain(admitted.iter())
            .filter(|seq| !after.contains(seq))
            .copied()
            .collect();
        pruned.sort_unstable();
        Ok(Settlement { admitted, pruned })
    }

    fn record_stop(&mut self, _stop: ProductStop) -> Result<(), FrontierError> {
        Ok(())
    }

    fn retained(&self) -> Vec<RetainedEntry> {
        let mut entries: Vec<RetainedEntry> = self
            .frontier
            .entries()
            .iter()
            .map(|entry| RetainedEntry {
                seq: entry.id,
                times_selected: entry.times_selected,
                score_bits: entry.score.to_bits(),
            })
            .collect();
        entries.sort_by_key(|entry| entry.seq);
        entries
    }

    fn len(&self) -> usize {
        self.frontier.len()
    }
}
