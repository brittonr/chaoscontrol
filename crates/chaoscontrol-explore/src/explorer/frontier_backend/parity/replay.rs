//! Trace-pair parity check.
//!
//! Takes the `frontier-trace.jsonl` of a legacy run and of a Campaign run of
//! the same seed and configuration, replays each run's recorded observations
//! and selection draws through the OTHER policy's backend, and compares the
//! replayed structural decisions with the recorded ones after every round.
//! Each run is compared only against its own recorded observations, so live
//! coverage drift between the two runs (KVM seed repeatability is not
//! established) is a recorded non-claim, not a comparison input.

use super::super::trace::{
    parse_trace, TraceBranch, TraceDraws, TraceParseError, TraceProposal, TraceRecord, TraceRound,
    OUTCOME_EXECUTED, OUTCOME_SKIPPED, TRACE_SCHEMA,
};
use super::super::{EntryProposal, RetainedEntry, RoundMode, SelectionUse};
use super::drive::{
    drive_policy, BranchScript, DriveError, ParentMoment, ParentOrigin, RngPosition, RoundDecision,
    RoundEntropy, RoundOutcome, RoundScript, RunDecisions, RunScript,
};
use super::report::{
    compare_runs, EvidenceQuestion, ParityDrift, ParityReport, PolicySide, SideRole,
};
use crate::campaign_adapter::BranchSlot;
use crate::explorer::{ExplorationMode, ExplorationStop, ExplorerConfig, FrontierPolicy};
use crate::marker_branching::MarkerFrontierMetadata;
use rand::RngCore;
use std::collections::{BTreeMap, VecDeque};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

const MAX_PARITY_TRACE_BYTES: u64 = 64 * 1024 * 1024;

/// Schema of a parity receipt; product evidence admission refuses it as
/// structural (`chaoscontrol_evidence::campaign_evidence_boundary`).
pub const PARITY_RECEIPT_SCHEMA: &str =
    chaoscontrol_evidence::campaign_evidence_boundary::FRONTIER_PARITY_RECEIPT_SCHEMA;
/// Selection-budget units a recorded selection consumed (one round).
const RECORDED_SELECTION_BUDGET_UNITS: u64 = 1;
/// Low bits `rand` 0.8 discards from one `next_u64` when drawing an `f64`.
const UNIT_DRAW_DISCARDED_BITS: u32 = 11;
/// `2^53` as an exact `f64`: unit draws are `k * 2^-53`.
const UNIT_DRAW_SCALE: f64 = 9_007_199_254_740_992.0;
/// Marker identity replayed for a recorded marker proposal. Candidate identity
/// binds the entry sequence number, so repeated markers stay distinct.
const REPLAY_MARKER_IDENTITY: &str = "chaoscontrol.frontier-parity.replayed-marker";

/// What a parity receipt does not claim.
pub const PARITY_NON_CLAIMS: [&str; 8] = [
    "live coverage drift between the two runs is not compared; each run is replayed only against its own recorded observations",
    "time and memory are not measured",
    "structural frontier decisions only; not VM, fault, assertion, finding, replay, minimization, or release evidence",
    "the replayed Campaign backend writes a scratch journal without fsync; publication durability is not exercised",
    "legacy corpus recycling and the empty-frontier live-controller continuation are not replayed",
    "the operator signal is taken from the recorded stop, not re-observed",
    "input-tree alternative draws are not replayed; only selection draws are",
    "resumed runs (a trace with more than one header) are not supported",
];

/// Which run of the pair a trace is.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceRole {
    /// The legacy-policy run.
    Legacy,
    /// The Campaign-policy run.
    Campaign,
}

impl TraceRole {
    const fn policy(self) -> FrontierPolicy {
        match self {
            Self::Legacy => FrontierPolicy::LegacyDiagnostic,
            Self::Campaign => FrontierPolicy::Campaign,
        }
    }

    const fn other_policy(self) -> FrontierPolicy {
        match self {
            Self::Legacy => FrontierPolicy::Campaign,
            Self::Campaign => FrontierPolicy::LegacyDiagnostic,
        }
    }

    const fn replay_case(self) -> &'static str {
        match self {
            Self::Legacy => "legacy-run-replayed-through-campaign",
            Self::Campaign => "campaign-run-replayed-through-legacy",
        }
    }
}

/// One input trace.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct TraceInput {
    /// Role in the pair.
    pub role: TraceRole,
    /// Path as given.
    pub path: String,
    /// BLAKE3 of the file bytes (hex).
    pub blake3: String,
    /// Trace records.
    pub records: usize,
    /// Policy the header names.
    pub policy: FrontierPolicy,
}

/// Configuration both runs share.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct PairConfig {
    /// Exploration seed.
    pub seed: u64,
    /// Round bound.
    pub max_rounds: u64,
    /// Frontier bound.
    pub max_frontier: usize,
    /// Branches per round.
    pub branch_factor: usize,
    /// Exploration mode.
    pub mode: String,
    /// Stale-round limit.
    pub stale_round_limit: u64,
}

/// Parity of one recorded run replayed through the other policy.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct RunParity {
    /// Policy that recorded the run.
    pub recorded_policy: FrontierPolicy,
    /// Policy the observations were replayed through.
    pub replayed_through: FrontierPolicy,
    /// Recorded (left) versus replayed (right) decisions.
    pub report: ParityReport,
}

/// Parity receipt of one trace pair.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct FrontierParityReceipt {
    /// [`PARITY_RECEIPT_SCHEMA`].
    pub schema: String,
    /// Input traces.
    pub inputs: Vec<TraceInput>,
    /// Shared configuration.
    pub config: PairConfig,
    /// One entry per recorded run.
    pub runs: Vec<RunParity>,
    /// Every decision field of every run agrees.
    pub decision_fields_equal: bool,
    /// Semantic work of every run, measured separately from decision fields.
    pub semantic_work: EvidenceQuestion,
    /// Wall-clock time: not measured.
    pub time: EvidenceQuestion,
    /// Memory: not measured.
    pub memory: EvidenceQuestion,
    /// Decision fields agree for both runs.
    pub cutover_ready: bool,
    /// What this receipt does not claim.
    pub non_claims: Vec<String>,
}

impl FrontierParityReceipt {
    /// Cutover readiness: every run's decision fields agree.
    ///
    /// # Errors
    /// Returns the first run's [`ParityDrift`].
    pub fn cutover_readiness(&self) -> Result<(), ParityDrift> {
        self.runs
            .iter()
            .try_for_each(|run| run.report.cutover_readiness())
    }
}

/// A trace pair could not be checked.
#[derive(Debug)]
pub enum ParityCheckError {
    /// A trace could not be read.
    Read {
        /// Trace path.
        path: PathBuf,
        /// I/O failure.
        source: io::Error,
    },
    /// A trace line is not a schema record.
    Parse {
        /// Trace path.
        path: PathBuf,
        /// Decoder failure.
        source: TraceParseError,
    },
    /// A trace does not have the header, root, rounds, stop shape.
    Shape {
        /// Trace path.
        path: PathBuf,
        /// What is wrong.
        reason: &'static str,
    },
    /// A trace's header names another policy than its role.
    PolicyRole {
        /// Trace path.
        path: PathBuf,
        /// Expected policy.
        expected: FrontierPolicy,
        /// Header policy.
        found: FrontierPolicy,
    },
    /// The two runs differ in configuration.
    ConfigMismatch {
        /// First differing header field.
        field: &'static str,
    },
    /// A recorded round is internally inconsistent.
    InconsistentRound {
        /// Trace path.
        path: PathBuf,
        /// Loop round.
        round: u64,
        /// What is wrong.
        reason: &'static str,
    },
    /// The scratch directory for the replayed Campaign journal failed.
    Scratch(io::Error),
    /// The replay driver failed.
    Drive(DriveError),
    /// The receipt could not be written.
    ReceiptWrite {
        /// Receipt path.
        path: PathBuf,
        /// I/O failure.
        source: io::Error,
    },
}

impl std::fmt::Display for ParityCheckError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::Parse { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::Shape { path, reason } => write!(formatter, "{}: {reason}", path.display()),
            Self::PolicyRole {
                path,
                expected,
                found,
            } => write!(
                formatter,
                "{}: header policy {found} where {expected} is required",
                path.display()
            ),
            Self::ConfigMismatch { field } => {
                write!(formatter, "the two runs differ in header field {field}")
            }
            Self::InconsistentRound {
                path,
                round,
                reason,
            } => write!(formatter, "{}: round {round}: {reason}", path.display()),
            Self::Scratch(source) => write!(formatter, "scratch journal directory: {source}"),
            Self::Drive(source) => write!(formatter, "replay: {source}"),
            Self::ReceiptWrite { path, source } => {
                write!(formatter, "{}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for ParityCheckError {}

/// Header facts of one recorded run.
#[derive(Clone, Debug, Eq, PartialEq)]
struct RecordedHeader {
    policy: FrontierPolicy,
    config: PairConfig,
}

/// One recorded run.
#[derive(Clone, Debug)]
struct RecordedRun {
    path: PathBuf,
    header: RecordedHeader,
    root_proposals: Vec<TraceProposal>,
    root_retained: Vec<RetainedEntry>,
    rounds: Vec<TraceRound>,
    stop: ExplorationStop,
    rounds_completed: u64,
}

fn shape(path: &Path, reason: &'static str) -> ParityCheckError {
    ParityCheckError::Shape {
        path: path.to_path_buf(),
        reason,
    }
}

fn inconsistent(path: &Path, round: u64, reason: &'static str) -> ParityCheckError {
    ParityCheckError::InconsistentRound {
        path: path.to_path_buf(),
        round,
        reason,
    }
}

fn parse_mode(name: &str) -> Option<ExplorationMode> {
    match name {
        "fault-schedule" => Some(ExplorationMode::FaultSchedule),
        "input-tree" => Some(ExplorationMode::InputTree),
        "hybrid" => Some(ExplorationMode::Hybrid),
        _ => None,
    }
}

fn parse_slot(name: &str) -> Option<BranchSlot> {
    if name == "probe" {
        return Some(BranchSlot::Probe);
    }
    let (kind, index) = name.split_once(':')?;
    let index: u32 = index.parse().ok()?;
    match kind {
        "variant" => Some(BranchSlot::Variant(index)),
        "alternative" => Some(BranchSlot::Alternative(index)),
        _ => None,
    }
}

/// Splits a trace into its records, requiring exactly one run.
fn recorded_run(path: &Path, records: Vec<TraceRecord>) -> Result<RecordedRun, ParityCheckError> {
    let mut records = records.into_iter();
    let Some(TraceRecord::Header {
        schema,
        policy,
        seed,
        max_rounds,
        max_frontier,
        branch_factor,
        stale_round_limit,
        mode,
        journal: _,
    }) = records.next()
    else {
        return Err(shape(path, "the first record is not a header"));
    };
    if schema != TRACE_SCHEMA {
        return Err(shape(path, "unknown trace schema"));
    }
    if parse_mode(&mode).is_none() {
        return Err(shape(path, "unknown exploration mode"));
    }
    let Some(TraceRecord::Root {
        proposals: root_proposals,
        retained: root_retained,
    }) = records.next()
    else {
        return Err(shape(path, "the second record is not the root"));
    };
    let mut rounds = Vec::new();
    let mut stop = None;
    for record in records {
        if stop.is_some() {
            return Err(shape(path, "records follow the stop"));
        }
        match record {
            TraceRecord::Round(round) => {
                if usize::try_from(round.round).ok() != Some(rounds.len()) {
                    return Err(shape(path, "rounds are not numbered 0, 1, 2, ..."));
                }
                rounds.push(round);
            }
            TraceRecord::Stop {
                stop: class,
                rounds_completed,
            } => stop = Some((class, rounds_completed)),
            TraceRecord::Header { .. } => {
                return Err(shape(
                    path,
                    "the trace holds more than one run; resumed runs are not supported",
                ))
            }
            TraceRecord::Root { .. } => return Err(shape(path, "a second root record")),
        }
    }
    let Some((stop, rounds_completed)) = stop else {
        return Err(shape(path, "the trace has no stop record"));
    };
    if usize::try_from(rounds_completed).ok() != Some(rounds.len()) {
        return Err(shape(
            path,
            "the stop's rounds_completed differs from the rounds",
        ));
    }
    Ok(RecordedRun {
        path: path.to_path_buf(),
        header: RecordedHeader {
            policy,
            config: PairConfig {
                seed,
                max_rounds,
                max_frontier,
                branch_factor,
                mode,
                stale_round_limit,
            },
        },
        root_proposals,
        root_retained,
        rounds,
        stop,
        rounds_completed,
    })
}

fn proposal(recorded: TraceProposal) -> EntryProposal {
    EntryProposal {
        score: f64::from_bits(recorded.score_bits),
        marker: recorded.marker.then(|| MarkerFrontierMetadata {
            marker_identity: REPLAY_MARKER_IDENTITY.to_owned(),
            owner: REPLAY_MARKER_IDENTITY.to_owned(),
            state_ref: None,
            logical_position_ref: None,
            run_id: 0,
            observed_tick: 0,
        }),
        assertion: recorded.assertion,
    }
}

fn branch_script(
    path: &Path,
    round: u64,
    recorded: &TraceBranch,
) -> Result<BranchScript, ParityCheckError> {
    let slot = parse_slot(&recorded.slot)
        .ok_or_else(|| inconsistent(path, round, "unknown branch slot"))?;
    let script = BranchScript {
        slot,
        new_edges: recorded.new_edges,
        snapshot: recorded.snapshot,
        new_findings: recorded.new_bugs,
        proposals: recorded.proposals.iter().cloned().map(proposal).collect(),
    };
    if script.retained() == script.proposals.is_empty() {
        return Err(inconsistent(
            path,
            round,
            "a branch has proposals exactly when it has new edges and a snapshot",
        ));
    }
    Ok(script)
}

impl RecordedRun {
    /// The recorded observations as a script for `policy`.
    fn script(
        &self,
        policy: FrontierPolicy,
        scratch: &Path,
    ) -> Result<RunScript, ParityCheckError> {
        let header = &self.header.config;
        let exploration_mode = parse_mode(&header.mode)
            .ok_or_else(|| shape(&self.path, "unknown exploration mode"))?;
        let config = ExplorerConfig {
            seed: header.seed,
            max_rounds: header.max_rounds,
            max_frontier: header.max_frontier,
            branch_factor: header.branch_factor,
            exploration_mode,
            stale_round_limit: header.stale_round_limit,
            frontier_policy: policy,
            output_dir: Some(scratch.display().to_string()),
            ..ExplorerConfig::default()
        };
        let signal_round = (self.stop == ExplorationStop::Interrupted)
            .then(|| self.rounds_completed.checked_sub(1))
            .flatten();
        let rounds = self
            .rounds
            .iter()
            .map(|round| {
                let branches = round
                    .branches
                    .iter()
                    .map(|branch| branch_script(&self.path, round.round, branch))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(RoundScript {
                    branches,
                    choice_histories: Vec::new(),
                    signal: signal_round == Some(round.round),
                })
            })
            .collect::<Result<Vec<_>, ParityCheckError>>()?;
        Ok(RunScript {
            config,
            root: self.root_proposals.iter().cloned().map(proposal).collect(),
            rounds,
        })
    }

    /// The recorded decisions, with parent moments derived from the
    /// admission order of the recorded proposals.
    fn decisions(&self) -> Result<RunDecisions, ParityCheckError> {
        let mut origins: BTreeMap<u64, ParentMoment> = BTreeMap::new();
        for (seq, _) in (0_u64..).zip(&self.root_proposals) {
            origins.insert(
                seq,
                ParentMoment {
                    origin: ParentOrigin::Root,
                    depth: 0,
                },
            );
        }
        let mut selections = 0_u64;
        let mut rounds = Vec::with_capacity(self.rounds.len());
        for round in &self.rounds {
            let mode = RoundMode::from_trace_name(&round.mode)
                .ok_or_else(|| inconsistent(&self.path, round.round, "unknown round mode"))?;
            let outcome = match round.outcome.as_str() {
                OUTCOME_EXECUTED => RoundOutcome::Executed,
                OUTCOME_SKIPPED => RoundOutcome::Skipped,
                _ => {
                    return Err(inconsistent(
                        &self.path,
                        round.round,
                        "unknown round outcome",
                    ))
                }
            };
            if round.selection.is_some() {
                selections = selections.saturating_add(1);
            }
            let selected_parent = round
                .selection
                .and_then(|selection| origins.get(&selection.seq).copied());
            let parent = match outcome {
                RoundOutcome::Executed => selected_parent,
                RoundOutcome::Skipped => None,
            };
            let child_depth = selected_parent
                .map_or(0, |parent| parent.depth)
                .saturating_add(1);
            let mut admitted = round.admitted.iter().copied();
            for (branch_index, branch) in (0_u32..).zip(&round.branches) {
                for _ in &branch.proposals {
                    let seq = admitted.next().ok_or_else(|| {
                        inconsistent(
                            &self.path,
                            round.round,
                            "fewer admitted entries than proposals",
                        )
                    })?;
                    origins.insert(
                        seq,
                        ParentMoment {
                            origin: ParentOrigin::Branch {
                                round: round.round,
                                branch: branch_index,
                            },
                            depth: child_depth,
                        },
                    );
                }
            }
            if admitted.next().is_some() {
                return Err(inconsistent(
                    &self.path,
                    round.round,
                    "more admitted entries than proposals",
                ));
            }
            rounds.push(RoundDecision {
                round: round.round,
                mode,
                outcome,
                draws: round.draws,
                entropy_mismatch: false,
                selection: round.selection,
                parent,
                admitted: round.admitted.clone(),
                pruned: round.pruned.clone(),
                retained: round.retained.clone(),
                selection_use: SelectionUse {
                    selections,
                    budget_units: selections.saturating_mul(RECORDED_SELECTION_BUDGET_UNITS),
                },
                work: round.work,
                rng_position: None,
            });
        }
        Ok(RunDecisions {
            root: self.root_retained.clone(),
            rounds,
            stop: Some(self.stop),
            rounds_completed: self.rounds_completed,
            failure: None,
        })
    }

    /// Recorded selection draws with the frontier size each was drawn over.
    fn entropy(&self) -> Result<ReplayEntropy, ParityCheckError> {
        let mut rounds = Vec::with_capacity(self.rounds.len());
        let mut len = self.root_retained.len();
        for round in &self.rounds {
            let words = match round.draws {
                None => Vec::new(),
                Some(draws) => draw_words(&self.path, round.round, draws, len)?,
            };
            rounds.push(words);
            len = round.retained.len();
        }
        Ok(ReplayEntropy {
            rounds,
            queue: VecDeque::new(),
            overdrawn: false,
        })
    }
}

/// `next_u64` words that make `rand` 0.8 draw exactly `draws` over `len` entries:
/// `gen::<f64>()` keeps the top 53 bits, and `gen_range(0..len)` takes the high
/// word of `v * len` when the low word is inside the acceptance zone.
fn draw_words(
    path: &Path,
    round: u64,
    draws: TraceDraws,
    len: usize,
) -> Result<Vec<u64>, ParityCheckError> {
    let unit = f64::from_bits(draws.unit_bits);
    let scaled = unit * UNIT_DRAW_SCALE;
    if !(0.0..1.0).contains(&unit) || scaled.fract() != 0.0 {
        return Err(inconsistent(
            path,
            round,
            "a unit draw is not a rand 0.8 f64",
        ));
    }
    // `scaled` is an exact integer below 2^53.
    let unit_word = (scaled as u64) << UNIT_DRAW_DISCARDED_BITS;
    let mut words = vec![unit_word];
    if let Some(index) = draws.index {
        let range = u128::try_from(len).map_err(|_| {
            inconsistent(path, round, "the frontier size does not fit a draw range")
        })?;
        if u128::from(index) >= range {
            return Err(inconsistent(
                path,
                round,
                "an index draw exceeds the frontier",
            ));
        }
        let word = (u128::from(index) << u64::BITS).div_ceil(range);
        words.push(
            u64::try_from(word)
                .map_err(|_| inconsistent(path, round, "an index draw exceeds the frontier"))?,
        );
    }
    Ok(words)
}

/// Serves each round's recorded selection words and flags a backend that
/// draws more or fewer.
struct ReplayEntropy {
    rounds: Vec<Vec<u64>>,
    queue: VecDeque<u64>,
    overdrawn: bool,
}

impl RngCore for ReplayEntropy {
    fn next_u32(&mut self) -> u32 {
        // `rand` 0.8 draws every selection value from `next_u64`.
        (self.next_u64() >> u32::BITS) as u32
    }

    fn next_u64(&mut self) -> u64 {
        self.queue.pop_front().unwrap_or_else(|| {
            self.overdrawn = true;
            0
        })
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        for chunk in dest.chunks_mut(std::mem::size_of::<u64>()) {
            let word = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&word[..chunk.len()]);
        }
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}

impl RoundEntropy for ReplayEntropy {
    fn selection(&mut self, round: u64) -> &mut dyn RngCore {
        self.queue = usize::try_from(round)
            .ok()
            .and_then(|index| self.rounds.get(index))
            .map(|words| words.iter().copied().collect())
            .unwrap_or_default();
        self.overdrawn = false;
        self
    }

    fn selection_mismatch(&mut self, _round: u64) -> bool {
        self.overdrawn || !self.queue.is_empty()
    }

    fn product(&mut self) -> &mut dyn RngCore {
        self
    }

    fn position(&self) -> Option<RngPosition> {
        None
    }
}

/// Replays a recorded run through `policy`.
fn replay(
    recorded: &RecordedRun,
    policy: FrontierPolicy,
) -> Result<RunDecisions, ParityCheckError> {
    let scratch = tempfile::tempdir().map_err(ParityCheckError::Scratch)?;
    let script = recorded.script(policy, scratch.path())?;
    let mut entropy = recorded.entropy()?;
    drive_policy(policy, &script, &mut entropy, scratch.path(), None)
        .map_err(ParityCheckError::Drive)
}

fn load(
    role: TraceRole,
    path: &Path,
    bytes: &[u8],
) -> Result<(TraceInput, RecordedRun), ParityCheckError> {
    let text = std::str::from_utf8(bytes).map_err(|error| ParityCheckError::Read {
        path: path.to_path_buf(),
        source: io::Error::new(io::ErrorKind::InvalidData, error),
    })?;
    let records = parse_trace(text).map_err(|source| ParityCheckError::Parse {
        path: path.to_path_buf(),
        source,
    })?;
    let count = records.len();
    let run = recorded_run(path, records)?;
    if run.header.policy != role.policy() {
        return Err(ParityCheckError::PolicyRole {
            path: path.to_path_buf(),
            expected: role.policy(),
            found: run.header.policy,
        });
    }
    let input = TraceInput {
        role,
        path: path.display().to_string(),
        blake3: blake3::hash(bytes).to_hex().to_string(),
        records: count,
        policy: run.header.policy,
    };
    Ok((input, run))
}

fn first_config_difference(left: &PairConfig, right: &PairConfig) -> Option<&'static str> {
    if left.seed != right.seed {
        return Some("seed");
    }
    if left.max_rounds != right.max_rounds {
        return Some("max_rounds");
    }
    if left.max_frontier != right.max_frontier {
        return Some("max_frontier");
    }
    if left.branch_factor != right.branch_factor {
        return Some("branch_factor");
    }
    if left.mode != right.mode {
        return Some("mode");
    }
    if left.stale_round_limit != right.stale_round_limit {
        return Some("stale_round_limit");
    }
    None
}

/// Checks one trace pair given as bytes (`legacy` then `campaign`).
///
/// # Errors
/// Returns [`ParityCheckError`] for unreadable, malformed, mismatched, or
/// internally inconsistent traces; decision drift is reported in the receipt.
pub fn check_trace_bytes(
    legacy: (&Path, &[u8]),
    campaign: (&Path, &[u8]),
) -> Result<FrontierParityReceipt, ParityCheckError> {
    let loaded = [
        load(TraceRole::Legacy, legacy.0, legacy.1)?,
        load(TraceRole::Campaign, campaign.0, campaign.1)?,
    ];
    let config = loaded[0].1.header.config.clone();
    if let Some(field) = first_config_difference(&config, &loaded[1].1.header.config) {
        return Err(ParityCheckError::ConfigMismatch { field });
    }
    let mut runs = Vec::with_capacity(loaded.len());
    for (input, recorded) in &loaded {
        let recorded_decisions = recorded.decisions()?;
        let through = input.role.other_policy();
        let replayed = replay(recorded, through)?;
        let report = compare_runs(
            input.role.replay_case(),
            PolicySide {
                policy: input.role.policy(),
                role: SideRole::Recorded,
            },
            &recorded_decisions,
            PolicySide {
                policy: through,
                role: SideRole::Replayed,
            },
            &replayed,
        );
        runs.push(RunParity {
            recorded_policy: input.role.policy(),
            replayed_through: through,
            report,
        });
    }
    let decision_fields_equal = runs.iter().all(|run| run.report.decision_fields.equal);
    let work_equal = runs
        .iter()
        .all(|run| run.report.semantic_work.answer == EvidenceQuestion::Measured { equal: true });
    Ok(FrontierParityReceipt {
        schema: PARITY_RECEIPT_SCHEMA.to_owned(),
        inputs: loaded.into_iter().map(|(input, _)| input).collect(),
        config,
        runs,
        decision_fields_equal,
        semantic_work: EvidenceQuestion::Measured { equal: work_equal },
        time: EvidenceQuestion::NotMeasured,
        memory: EvidenceQuestion::NotMeasured,
        cutover_ready: decision_fields_equal,
        non_claims: PARITY_NON_CLAIMS
            .iter()
            .map(|claim| (*claim).to_owned())
            .collect(),
    })
}

/// Checks the trace files of a legacy run and a Campaign run.
///
/// # Errors
/// As [`check_trace_bytes`], plus unreadable files.
pub fn check_trace_pair(
    legacy: &Path,
    campaign: &Path,
) -> Result<FrontierParityReceipt, ParityCheckError> {
    let legacy_bytes = read_trace_bytes(legacy)?;
    let campaign_bytes = read_trace_bytes(campaign)?;
    check_trace_bytes((legacy, &legacy_bytes), (campaign, &campaign_bytes))
}

fn read_trace_bytes(path: &Path) -> Result<Vec<u8>, ParityCheckError> {
    use std::os::unix::fs::OpenOptionsExt;

    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|source| ParityCheckError::Read {
            path: path.to_path_buf(),
            source,
        })?;
    let metadata = file.metadata().map_err(|source| ParityCheckError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    if !metadata.file_type().is_file() {
        return Err(ParityCheckError::Read {
            path: path.to_path_buf(),
            source: io::Error::new(io::ErrorKind::InvalidData, "trace is not a regular file"),
        });
    }
    if metadata.len() > MAX_PARITY_TRACE_BYTES {
        return Err(ParityCheckError::Read {
            path: path.to_path_buf(),
            source: io::Error::new(io::ErrorKind::InvalidData, "trace exceeds the byte limit"),
        });
    }
    let mut bytes = Vec::new();
    file.take(MAX_PARITY_TRACE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| ParityCheckError::Read {
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() as u64 > MAX_PARITY_TRACE_BYTES {
        return Err(ParityCheckError::Read {
            path: path.to_path_buf(),
            source: io::Error::new(io::ErrorKind::InvalidData, "trace exceeds the byte limit"),
        });
    }
    if bytes.len() as u64 != metadata.len() {
        return Err(ParityCheckError::Read {
            path: path.to_path_buf(),
            source: io::Error::new(io::ErrorKind::InvalidData, "trace changed while reading"),
        });
    }
    Ok(bytes)
}

/// Writes a receipt as pretty JSON to a new file and synchronizes it; never
/// replaces an existing receipt.
///
/// # Errors
/// Returns [`ParityCheckError::ReceiptWrite`] when the file exists or cannot
/// be written.
pub fn write_receipt(path: &Path, receipt: &FrontierParityReceipt) -> Result<(), ParityCheckError> {
    let failure = |source| ParityCheckError::ReceiptWrite {
        path: path.to_path_buf(),
        source,
    };
    let mut bytes = serde_json::to_vec_pretty(receipt).map_err(|error| failure(error.into()))?;
    bytes.push(b'\n');
    write_new_file(path, &bytes).map_err(failure)
}

fn write_new_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist_noclobber(path)
        .map_err(|error| error.error)?;
    std::fs::File::open(parent)?.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_publication_is_atomic_and_never_overwrites() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("nested/receipt.json");
        write_new_file(&path, b"{}\n").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"{}\n");
        assert!(write_new_file(&path, b"replacement").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"{}\n");
    }

    #[test]
    fn trace_reads_reject_length_changes() {
        let path = Path::new("/proc/self/cmdline");
        let error = check_trace_pair(path, path).unwrap_err();
        assert!(format!("{error:?}").contains("changed while reading"));
    }

    #[test]
    fn trace_pair_rejects_symlinks_and_oversized_files() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.jsonl");
        std::fs::write(&target, b"").unwrap();
        let link = root.path().join("link.jsonl");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(check_trace_pair(&link, &target).is_err());

        let oversized = root.path().join("oversized.jsonl");
        let file = std::fs::File::create(&oversized).unwrap();
        file.set_len(MAX_PARITY_TRACE_BYTES + 1).unwrap();
        assert!(check_trace_pair(&oversized, &target).is_err());
    }
}
