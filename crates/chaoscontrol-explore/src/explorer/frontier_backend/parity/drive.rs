//! VM-free driver of the frontier round protocol.
//!
//! [`drive`] runs one frontier backend through the same steps as
//! `Explorer::run`: root admission, then per round `begin_round` (selection
//! draws), `authorize`, product draws inside input-tree rounds, `finish_round`,
//! and the shared legacy post-round stop rule, then `record_stop`. Branch facts
//! come from a [`RunScript`] keyed by (round, branch), never by the selected
//! entry, so both policies see identical observations. Snapshots are structural
//! stand-ins whose `tick` names the branch that produced them; frontier
//! decisions never read snapshot contents, and the tag lets the comparison see
//! which parent moment a backend restores.

use super::super::trace::{
    trace_draws, trace_proposals, FrontierTrace, TraceDraws, TraceRecord, TraceSelection,
};
use super::super::{
    exploration_stop, product_stop, Authorization, BranchDigests, BranchOutcome, CampaignFrontier,
    EntryProposal, FindingFacts, FrontierBackend, FrontierError, LegacyFrontier, RetainedBranch,
    RetainedEntry, RootAdmission, RoundBegin, RoundContext, RoundMode, RoundObservation,
    SelectionUse, SelectionWork,
};
use crate::campaign_adapter::BranchSlot;
use crate::coverage::CoverageBitmap;
use crate::explorer::journal::{PublicationIo, SystemPublicationIo};
use crate::explorer::round_stops::{
    completed_every_round, next_stale_rounds, post_round_stop, PostRoundFacts,
};
use crate::explorer::{ExplorationStop, ExplorerConfig, FrontierPolicy};
use crate::input_tree::select_alternatives;
use chaoscontrol_fault::engine::{ChoiceRecord, EngineConfig, FaultEngine};
use chaoscontrol_fault::schedule::FaultSchedule;
use chaoscontrol_vmm::controller::{NetworkFabric, SimulationSnapshot};
use rand::RngCore;
use std::io;
use std::path::Path;

/// Structural tag of the bootstrap root snapshot.
const ROOT_TAG: u64 = 0;
/// Low tag bits that hold the branch index; the round sits above them.
const BRANCH_TAG_BITS: u32 = 16;
/// Branches one scripted round may hold (the branch index fits its tag bits).
pub(crate) const MAX_SCRIPTED_BRANCHES: usize = 1 << BRANCH_TAG_BITS;
/// VMs in a structural snapshot's network fabric.
const STRUCTURAL_VM_COUNT: usize = 1;
/// Network seed of a structural snapshot.
const STRUCTURAL_NETWORK_SEED: u64 = 0;
/// Memory base set every structural snapshot binds.
const STRUCTURAL_BASE_SET: [u8; 32] = [0; 32];

/// Which branch result produced a snapshot a selection restores.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ParentOrigin {
    /// The bootstrap root.
    Root,
    /// Branch `branch` of loop round `round`.
    Branch {
        /// Loop round.
        round: u64,
        /// Branch index within the round.
        branch: u32,
    },
}

impl ParentOrigin {
    fn tag(self) -> u64 {
        match self {
            Self::Root => ROOT_TAG,
            Self::Branch { round, branch } => {
                (round.saturating_add(1) << BRANCH_TAG_BITS) | u64::from(branch)
            }
        }
    }

    /// The origin a structural snapshot tag names; `None` for a tag no
    /// structural snapshot carries.
    fn from_tag(tag: u64) -> Option<Self> {
        if tag == ROOT_TAG {
            return Some(Self::Root);
        }
        let branch_mask = (1_u64 << BRANCH_TAG_BITS) - 1;
        Some(Self::Branch {
            round: (tag >> BRANCH_TAG_BITS).checked_sub(1)?,
            branch: u32::try_from(tag & branch_mask).ok()?,
        })
    }
}

/// The parent moment one selection restores.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub(crate) struct ParentMoment {
    /// Branch result whose snapshot is restored.
    pub(crate) origin: ParentOrigin,
    /// Product depth of the selected entry.
    pub(crate) depth: u32,
}

/// Scripted facts of one executed branch.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BranchScript {
    /// Branch position.
    pub(crate) slot: BranchSlot,
    /// New enriched coverage edges.
    pub(crate) new_edges: usize,
    /// Whether the branch produced a snapshot.
    pub(crate) snapshot: bool,
    /// New deduplicated findings.
    pub(crate) new_findings: usize,
    /// Frontier-entry proposals; pushed only for a retained branch.
    pub(crate) proposals: Vec<EntryProposal>,
}

impl BranchScript {
    /// The product retains a branch with new edges and a snapshot.
    pub(crate) const fn retained(&self) -> bool {
        self.new_edges > 0 && self.snapshot
    }
}

/// Scripted facts of one round.
#[derive(Clone, Debug, Default)]
pub(crate) struct RoundScript {
    /// Executed branches in order.
    pub(crate) branches: Vec<BranchScript>,
    /// Choice histories an input-tree round draws alternatives from.
    pub(crate) choice_histories: Vec<(usize, Vec<ChoiceRecord>)>,
    /// An operator signal arrives during the round.
    pub(crate) signal: bool,
}

/// A complete scripted run.
#[derive(Clone)]
pub(crate) struct RunScript {
    /// Exploration configuration both policies run under.
    pub(crate) config: ExplorerConfig,
    /// Bootstrap proposals in push order.
    pub(crate) root: Vec<EntryProposal>,
    /// Per-round facts, indexed by loop round.
    pub(crate) rounds: Vec<RoundScript>,
}

/// Position of a counted RNG after a round.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub(crate) struct RngPosition {
    /// Words consumed so far.
    pub(crate) words: u64,
    /// The next word the stream yields.
    pub(crate) next_word: u64,
}

/// Randomness one driven run consumes.
pub(crate) trait RoundEntropy {
    /// RNG the round's selection draws from.
    fn selection(&mut self, round: u64) -> &mut dyn RngCore;
    /// Whether the selection consumed a different number of draws than the
    /// entropy source holds for the round (replay only).
    fn selection_mismatch(&mut self, round: u64) -> bool;
    /// RNG product draws inside a round come from (input-tree alternatives).
    fn product(&mut self) -> &mut dyn RngCore;
    /// Stream position, when the source is a counted stream.
    fn position(&self) -> Option<RngPosition>;
}

/// How a round ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RoundOutcome {
    /// The round executed and settled.
    Executed,
    /// The published selection was closed without execution.
    Skipped,
}

/// Decision facts of one round.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RoundDecision {
    /// Loop round.
    pub(crate) round: u64,
    /// Sub-mode.
    pub(crate) mode: RoundMode,
    /// Executed or skipped.
    pub(crate) outcome: RoundOutcome,
    /// Selection draws the backend consumed.
    pub(crate) draws: Option<TraceDraws>,
    /// The backend consumed a different number of draws than the source held.
    pub(crate) entropy_mismatch: bool,
    /// Selected entry and explore flag.
    pub(crate) selection: Option<TraceSelection>,
    /// Parent moment the selection restores (`None` when nothing executed from one).
    pub(crate) parent: Option<ParentMoment>,
    /// Entries admitted, in push order.
    pub(crate) admitted: Vec<u64>,
    /// Entries pruned, ascending.
    pub(crate) pruned: Vec<u64>,
    /// Retained entries after the round, ascending by sequence.
    pub(crate) retained: Vec<RetainedEntry>,
    /// Selections and budget consumed after the round.
    pub(crate) selection_use: SelectionUse,
    /// Planning work of the selection.
    pub(crate) work: Option<SelectionWork>,
    /// RNG position after the round (counted streams only).
    pub(crate) rng_position: Option<RngPosition>,
}

/// A backend failure that ended a driven run.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct DriveFailure {
    /// Loop round, or `None` for root admission and stop recording.
    pub round: Option<u64>,
    /// Failure text.
    pub message: String,
}

/// Decisions of one driven or recorded run.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct RunDecisions {
    /// Retained entries after root admission.
    pub(crate) root: Vec<RetainedEntry>,
    /// Every round that ran, in order.
    pub(crate) rounds: Vec<RoundDecision>,
    /// Stop class of the run.
    pub(crate) stop: Option<ExplorationStop>,
    /// Rounds completed.
    pub(crate) rounds_completed: u64,
    /// Backend failure, if the run ended on one.
    pub(crate) failure: Option<DriveFailure>,
}

/// A driven run could not be written or scripted.
#[derive(Debug)]
pub enum DriveError {
    /// The trace could not be written.
    Trace(io::Error),
    /// A round holds more branches than a structural tag can name.
    TooManyBranches {
        /// Loop round.
        round: u64,
    },
}

impl std::fmt::Display for DriveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Trace(error) => write!(formatter, "frontier trace: {error}"),
            Self::TooManyBranches { round } => write!(
                formatter,
                "round {round} holds more than {MAX_SCRIPTED_BRANCHES} branches"
            ),
        }
    }
}

impl std::error::Error for DriveError {}

impl From<io::Error> for DriveError {
    fn from(error: io::Error) -> Self {
        Self::Trace(error)
    }
}

/// Journal publication for scratch parity runs: real staging and no-replace
/// renames, but no fsync. Parity compares decisions; durability is covered by
/// the publication-fence tests and is a recorded non-claim of parity receipts.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ScratchPublicationIo;

impl PublicationIo for ScratchPublicationIo {
    type Stage = <SystemPublicationIo as PublicationIo>::Stage;

    fn create_stage(&mut self, directory: &Path) -> io::Result<Self::Stage> {
        SystemPublicationIo.create_stage(directory)
    }

    fn write_payload(&mut self, stage: &mut Self::Stage, bytes: &[u8]) -> io::Result<()> {
        SystemPublicationIo.write_payload(stage, bytes)
    }

    fn sync_payload(&mut self, _stage: &mut Self::Stage) -> io::Result<()> {
        Ok(())
    }

    fn persist_no_replace(&mut self, stage: Self::Stage, destination: &Path) -> io::Result<()> {
        SystemPublicationIo.persist_no_replace(stage, destination)
    }

    fn create_directory(&mut self, path: &Path) -> io::Result<()> {
        SystemPublicationIo.create_directory(path)
    }

    fn sync_directory(&mut self, _path: &Path) -> io::Result<()> {
        Ok(())
    }
}

/// Opens `policy`'s backend for a scratch run; the Campaign backend admits the
/// configuration and journals under `scratch`.
///
/// # Errors
/// Returns the backend's admission or journal failure.
pub(crate) fn open_backend(
    policy: FrontierPolicy,
    config: &ExplorerConfig,
    scratch: &Path,
) -> Result<Box<dyn FrontierBackend>, FrontierError> {
    match policy {
        FrontierPolicy::LegacyDiagnostic => Ok(Box::new(LegacyFrontier::new(config.max_frontier))),
        FrontierPolicy::Campaign => Ok(Box::new(CampaignFrontier::open_with_io(
            config,
            scratch,
            None,
            ScratchPublicationIo,
        )?)),
    }
}

/// Opens `policy`'s backend under `scratch` and drives `script` through it. A
/// backend that refuses the configuration yields a run whose only fact is that
/// failure, so the comparison reports it as a drift.
///
/// # Errors
/// As [`drive`].
pub(crate) fn drive_policy(
    policy: FrontierPolicy,
    script: &RunScript,
    entropy: &mut dyn RoundEntropy,
    scratch: &Path,
    trace: Option<&mut FrontierTrace>,
) -> Result<RunDecisions, DriveError> {
    match open_backend(policy, &script.config, scratch) {
        Ok(mut backend) => drive(backend.as_mut(), script, entropy, trace),
        Err(error) => Ok(RunDecisions {
            failure: Some(DriveFailure {
                round: None,
                message: error.to_string(),
            }),
            ..RunDecisions::default()
        }),
    }
}

/// Structural snapshot whose tick names the branch that produced it.
fn structural_snapshot(origin: ParentOrigin) -> SimulationSnapshot {
    let engine = FaultEngine::new(EngineConfig::default());
    SimulationSnapshot {
        tick: origin.tag(),
        vm_snapshots: Vec::new(),
        network_state: NetworkFabric::new(STRUCTURAL_VM_COUNT, STRUCTURAL_NETWORK_SEED),
        fault_engine_snapshot: engine.snapshot(),
        vcpu_stall_until: Vec::new(),
        clock_freeze: Vec::new(),
        clock_jitter_bound: Vec::new(),
        memory_pressure: Vec::new(),
        process_fault_attempt: Vec::new(),
        pending_process_observations: Default::default(),
        fault_operation_sequence: 0,
    }
}

/// Distinct structural digests per branch result, so every child moment differs.
fn structural_digests(origin: ParentOrigin) -> BranchDigests {
    let tag = origin.tag();
    let digest = |domain: u8| {
        let mut bytes = [domain; 32];
        bytes[..8].copy_from_slice(&tag.to_le_bytes());
        bytes
    };
    BranchDigests {
        schedule: digest(1),
        variant: digest(2),
        coverage: digest(3),
        oracle: digest(4),
        total_ticks: tag,
    }
}

/// Root admission of scripted proposals over the structural root snapshot.
pub(crate) fn root_admission(proposals: &[EntryProposal]) -> RootAdmission {
    RootAdmission {
        snapshot: Some(structural_snapshot(ParentOrigin::Root)),
        coverage: CoverageBitmap::new(),
        schedule: FaultSchedule::new(),
        proposals: proposals.to_vec(),
        digests: Some(structural_digests(ParentOrigin::Root)),
        base_set_id: STRUCTURAL_BASE_SET,
    }
}

/// Distinct opaque findings, numbered across the run.
struct FindingSource {
    next: u64,
}

impl FindingSource {
    fn take(&mut self, count: usize) -> Vec<FindingFacts> {
        (0..count)
            .map(|_| {
                let key = self.next;
                self.next = self.next.saturating_add(1);
                let mut fingerprint = [0_u8; 32];
                fingerprint[..8].copy_from_slice(&key.to_le_bytes());
                FindingFacts {
                    dedup_key: key,
                    assertion_fingerprint: fingerprint,
                }
            })
            .collect()
    }
}

fn observation(
    round: u64,
    depth: u32,
    script: &RoundScript,
    findings: &mut FindingSource,
) -> Result<RoundObservation, DriveError> {
    let mut branches = Vec::with_capacity(script.branches.len());
    for (index, branch) in script.branches.iter().enumerate() {
        let branch_index = u32::try_from(index)
            .ok()
            .filter(|_| index < MAX_SCRIPTED_BRANCHES)
            .ok_or(DriveError::TooManyBranches { round })?;
        let origin = ParentOrigin::Branch {
            round,
            branch: branch_index,
        };
        branches.push(BranchOutcome {
            slot: branch.slot,
            new_edges: branch.new_edges,
            snapshot_present: branch.snapshot,
            findings: findings.take(branch.new_findings),
            retained: branch.retained().then(|| RetainedBranch {
                snapshot: structural_snapshot(origin),
                coverage: CoverageBitmap::new(),
                schedule: FaultSchedule::new(),
                depth,
                proposals: branch.proposals.clone(),
                digests: Some(structural_digests(origin)),
            }),
        });
    }
    Ok(RoundObservation { branches })
}

/// One round's contribution to the stop rule.
struct RoundFacts {
    decision: RoundDecision,
    new_edges: usize,
    new_findings: usize,
}

enum RoundStep {
    Ran(Box<RoundFacts>),
    Stopped(ExplorationStop),
}

/// Drives `backend` through `script`, writing root, round, and stop records to
/// `trace` when given (the caller writes the header).
///
/// Backend failures end the run and are recorded in [`RunDecisions::failure`];
/// the comparison reports them as a decision drift.
///
/// # Errors
/// Returns [`DriveError`] when the trace cannot be written or a round holds
/// more branches than a structural tag can name.
pub(crate) fn drive(
    backend: &mut dyn FrontierBackend,
    script: &RunScript,
    entropy: &mut dyn RoundEntropy,
    mut trace: Option<&mut FrontierTrace>,
) -> Result<RunDecisions, DriveError> {
    let config = &script.config;
    let mut run = RunDecisions::default();
    if let Err(error) = backend.admit_root(root_admission(&script.root)) {
        run.failure = Some(DriveFailure {
            round: None,
            message: error.to_string(),
        });
        return Ok(run);
    }
    run.root = backend.retained();
    if let Some(trace) = trace.as_deref_mut() {
        trace.write(&TraceRecord::Root {
            proposals: trace_proposals(&script.root),
            retained: run.root.clone(),
        })?;
    }
    let mut findings = FindingSource { next: 0 };
    let mut stale_rounds = 0_u64;
    let mut stop = None;
    for round in 0..config.max_rounds {
        let Some(round_script) = usize::try_from(round)
            .ok()
            .and_then(|index| script.rounds.get(index))
        else {
            run.failure = Some(DriveFailure {
                round: Some(round),
                message: "no scripted observations for the round".to_owned(),
            });
            break;
        };
        let step = drive_round(
            backend,
            config,
            round,
            round_script,
            entropy,
            &mut findings,
            trace.as_deref_mut(),
        );
        let facts = match step {
            Ok(RoundStep::Ran(facts)) => facts,
            Ok(RoundStep::Stopped(begin_stop)) => {
                stop = Some(begin_stop);
                break;
            }
            Err(RoundError::Backend(message)) => {
                run.failure = Some(DriveFailure {
                    round: Some(round),
                    message,
                });
                break;
            }
            Err(RoundError::Drive(error)) => return Err(error),
        };
        run.rounds.push(facts.decision);
        run.rounds_completed = run.rounds_completed.saturating_add(1);
        stale_rounds = next_stale_rounds(stale_rounds, facts.new_edges, facts.new_findings);
        stop = post_round_stop(PostRoundFacts {
            // Corpus recycling needs a live controller; VM-free runs never recycle.
            frontier_exhausted: backend.len() == 0,
            consecutive_stale_rounds: stale_rounds,
            stale_round_limit: config.stale_round_limit,
            round_findings: facts.new_findings,
            max_rounds: config.max_rounds,
            shutdown_requested: round_script.signal,
        });
        if stop.is_some() {
            break;
        }
    }
    if run.failure.is_some() {
        return Ok(run);
    }
    if stop.is_none() && completed_every_round(run.rounds_completed, config.max_rounds) {
        stop = Some(ExplorationStop::Completed);
    }
    if let Some(stop) = stop {
        if let Err(error) = backend.record_stop(product_stop(stop)) {
            run.failure = Some(DriveFailure {
                round: None,
                message: error.to_string(),
            });
            return Ok(run);
        }
        if let Some(trace) = trace {
            trace.finish(&TraceRecord::Stop {
                stop,
                rounds_completed: run.rounds_completed,
            })?;
        }
    }
    run.stop = stop;
    Ok(run)
}

enum RoundError {
    Backend(String),
    Drive(DriveError),
}

impl From<FrontierError> for RoundError {
    fn from(error: FrontierError) -> Self {
        Self::Backend(error.to_string())
    }
}

impl From<DriveError> for RoundError {
    fn from(error: DriveError) -> Self {
        Self::Drive(error)
    }
}

impl From<io::Error> for RoundError {
    fn from(error: io::Error) -> Self {
        Self::Drive(DriveError::Trace(error))
    }
}

fn drive_round(
    backend: &mut dyn FrontierBackend,
    config: &ExplorerConfig,
    round: u64,
    script: &RoundScript,
    entropy: &mut dyn RoundEntropy,
    findings: &mut FindingSource,
    trace: Option<&mut FrontierTrace>,
) -> Result<RoundStep, RoundError> {
    let mode = RoundMode::for_round(config.exploration_mode, round);
    let context = RoundContext { round, mode };
    let begin = backend.begin_round(entropy.selection(round), context)?;
    let entropy_mismatch = entropy.selection_mismatch(round);
    let planned = match begin {
        RoundBegin::Planned(planned) => planned,
        RoundBegin::Stop(stop) => return Ok(RoundStep::Stopped(exploration_stop(stop))),
    };
    let authorized = match backend.authorize(planned)? {
        Authorization::Execute(authorized) => *authorized,
        Authorization::Skipped(skipped) => {
            let retained = backend.retained();
            if let Some(trace) = trace {
                trace.write(&TraceRecord::skipped(&skipped, retained.clone()))?;
            }
            return Ok(RoundStep::Ran(Box::new(RoundFacts {
                decision: RoundDecision {
                    round,
                    mode,
                    outcome: RoundOutcome::Skipped,
                    draws: trace_draws(skipped.draws),
                    entropy_mismatch,
                    selection: Some(TraceSelection {
                        seq: skipped.seq,
                        explore: skipped.explored,
                    }),
                    parent: None,
                    admitted: Vec::new(),
                    pruned: Vec::new(),
                    retained,
                    selection_use: backend.selection_use(),
                    work: Some(skipped.work),
                    rng_position: entropy.position(),
                },
                new_edges: 0,
                new_findings: 0,
            })));
        }
    };
    if mode == RoundMode::InputTree && !script.choice_histories.is_empty() {
        // The product draws alternatives after the probe run, between
        // authorization and settlement.
        select_alternatives(
            &script.choice_histories,
            config.branch_factor.saturating_sub(1),
            &mut entropy.product(),
        );
    }
    let selected = authorized.selected().cloned();
    let parent = selected.as_ref().and_then(|selected| {
        ParentOrigin::from_tag(selected.snapshot.tick).map(|origin| ParentMoment {
            origin,
            depth: selected.depth,
        })
    });
    let child_depth = selected
        .as_ref()
        .map_or(0, |selected| selected.depth)
        .saturating_add(1);
    let observed = observation(round, child_depth, script, findings)?;
    let draws = authorized.draws();
    let work = authorized.work();
    let settlement = backend.finish_round(authorized, &observed)?;
    let retained = backend.retained();
    if let Some(trace) = trace {
        trace.write(&TraceRecord::executed(
            context,
            draws,
            work,
            selected.as_ref(),
            &observed,
            &settlement,
            retained.clone(),
        ))?;
    }
    let new_edges = script
        .branches
        .iter()
        .map(|branch| branch.new_edges)
        .fold(0, usize::saturating_add);
    let new_findings = script
        .branches
        .iter()
        .map(|branch| branch.new_findings)
        .fold(0, usize::saturating_add);
    Ok(RoundStep::Ran(Box::new(RoundFacts {
        decision: RoundDecision {
            round,
            mode,
            outcome: RoundOutcome::Executed,
            draws: trace_draws(draws),
            entropy_mismatch,
            selection: selected.as_ref().map(|selected| TraceSelection {
                seq: selected.seq,
                explore: selected.explored,
            }),
            parent,
            admitted: settlement.admitted,
            pruned: settlement.pruned,
            retained,
            selection_use: backend.selection_use(),
            work: draws.draws.map(|_| work),
            rng_position: entropy.position(),
        },
        new_edges,
        new_findings,
    })))
}
