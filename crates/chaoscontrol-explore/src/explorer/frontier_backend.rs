//! Frontier backends: the legacy in-process frontier and the Campaign-backed
//! candidate frontier behind one round protocol.
//!
//! Every exploration round is split into four steps so the publication fence
//! sits before any expansion effect:
//!
//! 1. [`FrontierBackend::begin_round`] draws the round's epsilon-greedy
//!    randomness in the exact legacy order and plans the selection. No effect
//!    besides the RNG draws happens.
//! 2. [`FrontierBackend::authorize`] makes the selection executable. The
//!    Campaign backend durably publishes the selection segment and checks
//!    Campaign's execution gate and the parent snapshot's eligibility first.
//!    Only an [`AuthorizedRound`] exposes the parent snapshot, so mutation,
//!    probe runs, snapshot restore, worker dispatch, and KVM execution cannot
//!    start without it.
//! 3. The Explorer shell executes the round (product-owned effects).
//! 4. [`FrontierBackend::finish_round`] settles the round: children, guidance,
//!    decay, and pruning. The Campaign backend commits the settlement segment
//!    before the shell writes `checkpoint.json`.

pub(crate) mod campaign;
pub(crate) mod digests;
#[cfg(test)]
mod leakage_tests;
pub(crate) mod legacy;
pub mod parity;
mod records;
mod session;
#[cfg(test)]
mod tests;
pub(crate) mod trace;

use crate::campaign_adapter::{AdapterError, BranchSlot, LegacyDraws, ProductStop};
use crate::coverage::CoverageBitmap;
use crate::explorer::journal::JournalError;
use crate::explorer::ExplorationStop;
use crate::marker_branching::MarkerFrontierMetadata;
use chaoscontrol_fault::schedule::FaultSchedule;
use chaoscontrol_vmm::controller::SimulationSnapshot;
use rand::RngCore;

pub(crate) use campaign::{CampaignFrontier, CheckpointBinding};
pub(crate) use legacy::LegacyFrontier;
pub use records::{ReplayError, ReplayFailure};

/// Sub-mode one round executes (hybrid alternates between them).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RoundMode {
    /// Fault-schedule mutation round.
    FaultSchedule,
    /// Input-tree probe and alternatives round.
    InputTree,
}

impl RoundMode {
    /// Stable trace name.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::FaultSchedule => "fault-schedule",
            Self::InputTree => "input-tree",
        }
    }

    /// Sub-mode of loop round `round` under `mode`: hybrid runs fault-schedule
    /// rounds at even indices and input-tree rounds at odd ones.
    pub(crate) const fn for_round(mode: crate::explorer::ExplorationMode, round: u64) -> Self {
        match mode {
            crate::explorer::ExplorationMode::FaultSchedule => Self::FaultSchedule,
            crate::explorer::ExplorationMode::InputTree => Self::InputTree,
            crate::explorer::ExplorationMode::Hybrid if round.is_multiple_of(2) => {
                Self::FaultSchedule
            }
            crate::explorer::ExplorationMode::Hybrid => Self::InputTree,
        }
    }

    /// Parses a stable trace name.
    pub(crate) fn from_trace_name(name: &str) -> Option<Self> {
        match name {
            "fault-schedule" => Some(Self::FaultSchedule),
            "input-tree" => Some(Self::InputTree),
            _ => None,
        }
    }
}

/// Position and sub-mode of one round.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RoundContext {
    /// Loop index of the round in this run (restarts at 0 on resume).
    pub(crate) round: u64,
    /// Sub-mode the round executes.
    pub(crate) mode: RoundMode,
}

/// One frontier proposal per logical marker/owner of a retained branch, else one.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EntryProposal {
    /// Legacy frontier score at proposal time.
    pub(crate) score: f64,
    /// Marker that created the opportunity, if any.
    pub(crate) marker: Option<MarkerFrontierMetadata>,
    /// Exact assertion-hit provenance, when an assertion created the opportunity.
    pub(crate) assertion: Option<crate::assertion_checkpoints::AssertionCheckpointMetadata>,
}

/// Product digests of one retained branch result; never snapshot bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchDigests {
    /// Digest of the branch fault schedule.
    pub(crate) schedule: [u8; 32],
    /// Digest of the branch schedule variant.
    pub(crate) variant: [u8; 32],
    /// Digest of the enriched branch coverage.
    pub(crate) coverage: [u8; 32],
    /// Digest of the branch oracle report.
    pub(crate) oracle: [u8; 32],
    /// Ticks the branch ran.
    pub(crate) total_ticks: u64,
    /// Tick at which the child snapshot was taken.
    pub(crate) snapshot_tick: u64,
}

/// A branch result with new edges and a restorable snapshot.
#[derive(Clone)]
pub(crate) struct RetainedBranch {
    /// Snapshot every proposal of the branch restores.
    pub(crate) snapshot: SimulationSnapshot,
    /// Enriched coverage of the branch.
    pub(crate) coverage: CoverageBitmap,
    /// Fault schedule of the branch.
    pub(crate) schedule: FaultSchedule,
    /// Product depth of the new entries.
    pub(crate) depth: u32,
    /// Frontier-entry proposals in push order.
    pub(crate) proposals: Vec<EntryProposal>,
    /// Digests the child moment binds (computed only under the Campaign policy).
    pub(crate) digests: Option<BranchDigests>,
}

/// Opaque identity of one new deduplicated finding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FindingFacts {
    /// Product dedup key.
    pub(crate) dedup_key: u64,
    /// Failed assertion fingerprint.
    pub(crate) assertion_fingerprint: [u8; 32],
}

/// Product facts of one executed branch.
#[derive(Clone)]
pub(crate) struct BranchOutcome {
    /// Branch position.
    pub(crate) slot: BranchSlot,
    /// New enriched coverage edges.
    pub(crate) new_edges: usize,
    /// Whether the branch produced a snapshot.
    pub(crate) snapshot_present: bool,
    /// New deduplicated findings of the branch.
    pub(crate) findings: Vec<FindingFacts>,
    /// Retained branch-end and assertion-checkpoint sources, in push order.
    pub(crate) retained: Vec<RetainedBranch>,
}

/// Product facts of one executed round, in branch order.
#[derive(Clone, Default)]
pub(crate) struct RoundObservation {
    /// Every executed branch.
    pub(crate) branches: Vec<BranchOutcome>,
}

/// Bootstrap result offered as the frontier root.
#[derive(Clone)]
pub(crate) struct RootAdmission {
    /// Bootstrap snapshot; required by the Campaign policy.
    pub(crate) snapshot: Option<SimulationSnapshot>,
    /// Bootstrap coverage.
    pub(crate) coverage: CoverageBitmap,
    /// Initial fault schedule.
    pub(crate) schedule: FaultSchedule,
    /// Root frontier-entry proposals in push order.
    pub(crate) proposals: Vec<EntryProposal>,
    /// Digests the root moment binds (`total_ticks` = bootstrap tick);
    /// computed only under the Campaign policy.
    pub(crate) digests: Option<BranchDigests>,
    /// Identity of the incremental-snapshot memory base set.
    pub(crate) base_set_id: [u8; 32],
}

/// The recorded selection randomness of one round.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RoundDraws {
    /// Legacy draws; `None` when the frontier was empty and nothing was drawn.
    pub(crate) draws: Option<LegacyDraws>,
}

/// Semantic planning work of one selection, measured by the backend that
/// planned it. Reported beside decision fields, never folded into them: equal
/// decisions do not imply equal work.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SelectionWork {
    /// Candidate records the planner inspected.
    pub(crate) candidates_examined: u64,
    /// Candidates eligible for the selection.
    pub(crate) eligible_candidates: u64,
}

/// Selections and selection budget one backend has consumed so far.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SelectionUse {
    /// Selections made (legacy) or published (Campaign).
    pub(crate) selections: u64,
    /// Selection-budget units consumed (legacy) or reserved (Campaign).
    pub(crate) budget_units: u64,
}

/// Outcome of [`FrontierBackend::begin_round`].
pub(crate) enum RoundBegin {
    /// A selection is planned and awaits authorization.
    Planned(PlannedRound),
    /// The policy stops before selecting.
    Stop(ProductStop),
}

/// A planned but not yet executable round.
pub(crate) struct PlannedRound {
    pub(super) context: RoundContext,
    pub(super) draws: RoundDraws,
    pub(super) work: SelectionWork,
    pub(super) plan: PlanDetail,
}

/// Backend-specific plan payload.
pub(super) enum PlanDetail {
    /// Legacy: vector position, or the clean-slate round of an empty frontier.
    Legacy { index: Option<usize> },
    /// Campaign: the planned selection and its recorded request.
    Campaign(Box<campaign::CampaignPlan>),
}

impl PlannedRound {
    /// Recorded selection draws.
    #[cfg(test)]
    pub(crate) const fn draws(&self) -> RoundDraws {
        self.draws
    }
}

/// The parent entry an authorized round expands.
#[derive(Clone)]
pub(crate) struct SelectedEntry {
    /// Product frontier sequence number (legacy entry id).
    pub(crate) seq: u64,
    /// Parent snapshot to restore.
    pub(crate) snapshot: SimulationSnapshot,
    /// Parent fault schedule.
    pub(crate) schedule: FaultSchedule,
    /// Parent product depth.
    pub(crate) depth: u32,
    /// Exact marker instance that created the parent candidate.
    pub(crate) marker: Option<MarkerFrontierMetadata>,
    /// Exact assertion-hit provenance, when one created the candidate.
    pub(crate) assertion: Option<crate::assertion_checkpoints::AssertionCheckpointMetadata>,
    /// Whether the epsilon draw explored.
    pub(crate) explored: bool,
}

/// An executable round. Only a backend's `authorize` constructs one, after the
/// publication fence passed, so every expansion effect is gated on it.
pub(crate) struct AuthorizedRound {
    context: RoundContext,
    draws: RoundDraws,
    work: SelectionWork,
    selected: Option<SelectedEntry>,
    detail: AuthorizedDetail,
}

/// Backend-specific authorization payload.
pub(super) enum AuthorizedDetail {
    /// Legacy rounds carry no publication.
    Legacy,
    /// Campaign rounds carry the published selection.
    Campaign(Box<campaign::PublishedSelection>),
}

impl AuthorizedRound {
    pub(super) const fn new(
        context: RoundContext,
        draws: RoundDraws,
        work: SelectionWork,
        selected: Option<SelectedEntry>,
        detail: AuthorizedDetail,
    ) -> Self {
        Self {
            context,
            draws,
            work,
            selected,
            detail,
        }
    }

    /// Round position and sub-mode.
    pub(crate) const fn context(&self) -> RoundContext {
        self.context
    }

    /// Recorded selection draws.
    pub(crate) const fn draws(&self) -> RoundDraws {
        self.draws
    }

    /// Planning work of the round's selection.
    pub(crate) const fn work(&self) -> SelectionWork {
        self.work
    }

    /// The parent entry; `None` only for the legacy clean-slate round.
    pub(crate) const fn selected(&self) -> Option<&SelectedEntry> {
        self.selected.as_ref()
    }
}

/// A published round that cannot execute: its parent snapshot became ineligible
/// after publication, so the selection was closed unobserved and retired.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SkippedRound {
    /// Round position and sub-mode.
    pub(crate) context: RoundContext,
    /// Recorded selection draws.
    pub(crate) draws: RoundDraws,
    /// Planning work of the published selection.
    pub(crate) work: SelectionWork,
    /// Sequence number of the retired entry.
    pub(crate) seq: u64,
    /// Whether Campaign's entropy ticket took the explore branch.
    pub(crate) explored: bool,
}

/// Outcome of [`FrontierBackend::authorize`].
pub(crate) enum Authorization {
    /// The round may execute.
    Execute(Box<AuthorizedRound>),
    /// The round was closed without execution.
    Skipped(SkippedRound),
}

/// One retained frontier entry as the trace reports it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetainedEntry {
    /// Product sequence number.
    pub(crate) seq: u64,
    /// Selections so far.
    pub(crate) times_selected: u32,
    /// IEEE-754 bits of the score.
    pub(crate) score_bits: u64,
}

/// Frontier state after one settled round.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Settlement {
    /// Sequence numbers assigned to the round's proposals, in push order.
    pub(crate) admitted: Vec<u64>,
    /// Sequence numbers pruned by this round, ascending.
    pub(crate) pruned: Vec<u64>,
}

/// Typed failure of a frontier backend.
#[derive(Debug)]
pub enum FrontierError {
    /// A pure adapter mapping or Campaign/Choregraph call failed.
    Adapter(AdapterError),
    /// The durable journal failed (including every publication failure class).
    Journal(JournalError),
    /// Campaign waits although the sequential shell settles every round; a
    /// wait is never exhaustion.
    UnexpectedWait(campaign_core::WaitReason),
    /// Campaign refused execution after the publication was committed.
    ExecutionRefused,
    /// The selection segment's sequence collided twice, or the rebuilt history
    /// differs from this writer's state.
    StaleControlBranch,
    /// A publication was not committed; the backend dropped its pending state
    /// and stopped admitting rounds.
    Halted,
    /// The incrementally applied Campaign state differs from the validated projection.
    StateDivergence,
    /// A round was requested before the root was admitted, or twice.
    Sequence(&'static str),
    /// A published selection names a candidate the product does not hold.
    UnknownCandidate,
    /// A derived bound does not fit its representation.
    BoundOverflow,
    /// The recovered journal cannot be replayed into Campaign history.
    Replay(records::ReplayError),
    /// A product value could not be encoded for a digest or journal record.
    Encoding(&'static str),
}

impl std::fmt::Display for FrontierError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Adapter(error) => write!(formatter, "{error}"),
            Self::Journal(error) => write!(formatter, "campaign journal: {error}"),
            Self::UnexpectedWait(reason) => write!(
                formatter,
                "campaign policy waits ({reason:?}) in the sequential shell; not frontier exhaustion"
            ),
            Self::ExecutionRefused => formatter.write_str(
                "campaign refused execution of the published selection",
            ),
            Self::StaleControlBranch => {
                formatter.write_str("campaign control branch is stale after one replan")
            }
            Self::Halted => formatter.write_str(
                "campaign frontier halted after an uncommitted publication",
            ),
            Self::StateDivergence => formatter
                .write_str("campaign state diverged from the validated control projection"),
            Self::Sequence(step) => write!(formatter, "frontier round protocol violated: {step}"),
            Self::UnknownCandidate => {
                formatter.write_str("published selection names an unknown frontier entry")
            }
            Self::BoundOverflow => formatter.write_str("frontier bound overflow"),
            Self::Replay(error) => write!(formatter, "campaign journal replay: {error}"),
            Self::Encoding(what) => write!(formatter, "cannot encode {what}"),
        }
    }
}

impl std::error::Error for FrontierError {}

impl From<AdapterError> for FrontierError {
    fn from(error: AdapterError) -> Self {
        Self::Adapter(error)
    }
}

impl From<campaign_core::CampaignError> for FrontierError {
    fn from(error: campaign_core::CampaignError) -> Self {
        Self::Adapter(AdapterError::Campaign(error))
    }
}

impl From<campaign_choregraph::AdapterError> for FrontierError {
    fn from(error: campaign_choregraph::AdapterError) -> Self {
        Self::Adapter(AdapterError::Choregraph(error))
    }
}

impl From<JournalError> for FrontierError {
    fn from(error: JournalError) -> Self {
        Self::Journal(error)
    }
}

impl From<records::ReplayError> for FrontierError {
    fn from(error: records::ReplayError) -> Self {
        Self::Replay(error)
    }
}

/// Product stop of one legacy stop class.
pub(crate) const fn product_stop(stop: ExplorationStop) -> ProductStop {
    match stop {
        ExplorationStop::FrontierExhausted => ProductStop::FrontierExhausted,
        ExplorationStop::CoveragePlateau => ProductStop::CoveragePlateau,
        ExplorationStop::BugFound => ProductStop::ShortRunFinding,
        ExplorationStop::Interrupted => ProductStop::Interrupted,
        ExplorationStop::Completed => ProductStop::Completed,
        ExplorationStop::ControlCapacityExhausted => ProductStop::ControlCapacityExhausted,
    }
}

/// Legacy stop class of one Campaign product stop.
pub(crate) const fn exploration_stop(stop: ProductStop) -> ExplorationStop {
    match stop {
        ProductStop::FrontierExhausted => ExplorationStop::FrontierExhausted,
        ProductStop::CoveragePlateau => ExplorationStop::CoveragePlateau,
        ProductStop::ShortRunFinding => ExplorationStop::BugFound,
        ProductStop::Interrupted => ExplorationStop::Interrupted,
        ProductStop::Completed | ProductStop::CompletedWithoutSelection => {
            ExplorationStop::Completed
        }
        ProductStop::ControlCapacityExhausted => ExplorationStop::ControlCapacityExhausted,
    }
}

/// The round protocol both frontier policies implement.
pub(crate) trait FrontierBackend {
    /// Admits the bootstrap result as the frontier root.
    fn admit_root(&mut self, root: RootAdmission) -> Result<(), FrontierError>;

    /// Draws the round's selection randomness in legacy order and plans it.
    ///
    /// The Explorer passes its ChaCha8 stream; parity replay passes recorded
    /// draws. Nothing else in the round draws from `rng` before `authorize`.
    fn begin_round(
        &mut self,
        rng: &mut dyn RngCore,
        context: RoundContext,
    ) -> Result<RoundBegin, FrontierError>;

    /// Passes the publication fence; only then may the round execute.
    fn authorize(&mut self, planned: PlannedRound) -> Result<Authorization, FrontierError>;

    /// Settles an executed round.
    fn finish_round(
        &mut self,
        round: AuthorizedRound,
        observation: &RoundObservation,
    ) -> Result<Settlement, FrontierError>;

    /// Records the run's stop.
    fn record_stop(&mut self, stop: ProductStop) -> Result<(), FrontierError>;

    /// Retained entries in ascending sequence order.
    fn retained(&self) -> Vec<RetainedEntry>;

    /// Number of retained entries.
    fn len(&self) -> usize;

    /// Selections and selection budget consumed so far.
    fn selection_use(&self) -> SelectionUse;
}
