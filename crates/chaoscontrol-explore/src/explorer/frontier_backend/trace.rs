//! Product `frontier-trace.jsonl`: one decision record per line, written by
//! both frontier policies so a later parity check can replay each run's
//! recorded observations through the other policy.
//!
//! Records: `header` (policy, seed, bounds, stale-round limit, journal epoch and
//! recovery facts), `root` (bootstrap proposals and the retained entries after
//! them), one `round` per round (recorded draws, selected entry and explore
//! flag, the selection's planning work, branch facts with score bits, admitted
//! and pruned entries, retained set with selection counts), and `stop`. The
//! trace is a product decision log, not evidence of VM behavior.

use super::{
    EntryProposal, RetainedEntry, RoundContext, RoundDraws, RoundObservation, SelectedEntry,
    SelectionWork, Settlement, SkippedRound,
};
use crate::campaign_adapter::BranchSlot;
use crate::explorer::{ExplorationMode, ExplorationStop, ExplorerConfig, FrontierPolicy};
use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::Path;

/// File name of the trace under `output_dir`.
pub(crate) const TRACE_FILE_NAME: &str = "frontier-trace.jsonl";
/// Schema of every trace record.
pub(crate) const TRACE_SCHEMA: &str = "chaoscontrol.frontier-trace.v1";
/// Outcome of an executed round.
pub(crate) const OUTCOME_EXECUTED: &str = "executed";
/// Outcome of a round closed without execution (ineligible snapshot).
pub(crate) const OUTCOME_SKIPPED: &str = "skipped";

/// Journal facts of a Campaign run's header.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TraceJournal {
    /// Epoch this run writes.
    pub(crate) epoch: u32,
    /// Epochs recovered at open.
    pub(crate) recovered_epochs: usize,
    /// Outstanding selections closed unobserved at open.
    pub(crate) closed_selections: usize,
}

/// Recorded selection draws.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TraceDraws {
    /// Bits of `gen::<f64>()`.
    pub(crate) unit_bits: u64,
    /// `gen_range(0..len)` on the explore branch.
    pub(crate) index: Option<u64>,
}

/// Selected entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TraceSelection {
    /// Product sequence number.
    pub(crate) seq: u64,
    /// Whether the epsilon draw explored.
    pub(crate) explore: bool,
}

/// One frontier-entry proposal.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TraceProposal {
    /// IEEE-754 bits of the score.
    pub(crate) score_bits: u64,
    /// Whether a marker created it.
    pub(crate) marker: bool,
}

/// One executed branch.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TraceBranch {
    /// `probe`, `variant:<k>`, or `alternative:<k>`.
    pub(crate) slot: String,
    /// New enriched coverage edges.
    pub(crate) new_edges: usize,
    /// Whether the branch produced a snapshot.
    pub(crate) snapshot: bool,
    /// New deduplicated findings.
    pub(crate) new_bugs: usize,
    /// Proposals pushed from this branch.
    pub(crate) proposals: Vec<TraceProposal>,
}

/// One round.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TraceRound {
    /// Loop index.
    pub(crate) round: u64,
    /// Sub-mode.
    pub(crate) mode: String,
    /// `executed` or `skipped`.
    pub(crate) outcome: String,
    /// Recorded draws (`None` when nothing was drawn).
    pub(crate) draws: Option<TraceDraws>,
    /// Selected entry (`None` for the legacy clean-slate round).
    pub(crate) selection: Option<TraceSelection>,
    /// Planning work of the selection (`None` when nothing was drawn).
    pub(crate) work: Option<SelectionWork>,
    /// Executed branches.
    pub(crate) branches: Vec<TraceBranch>,
    /// Sequence numbers admitted.
    pub(crate) admitted: Vec<u64>,
    /// Sequence numbers pruned.
    pub(crate) pruned: Vec<u64>,
    /// Retained entries after the round, ascending by sequence.
    pub(crate) retained: Vec<RetainedEntry>,
}

/// One trace line.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum TraceRecord {
    /// Run header.
    Header {
        /// [`TRACE_SCHEMA`].
        schema: String,
        /// Frontier policy.
        policy: FrontierPolicy,
        /// Exploration seed.
        seed: u64,
        /// Round bound.
        max_rounds: u64,
        /// Frontier bound.
        max_frontier: usize,
        /// Branches per round.
        branch_factor: usize,
        /// Consecutive stale rounds that stop the run (0 disables the stop).
        stale_round_limit: u64,
        /// Exploration mode.
        mode: String,
        /// Campaign journal facts (`None` for the legacy policy).
        journal: Option<TraceJournal>,
    },
    /// Bootstrap proposals and the retained entries after the root admission.
    Root {
        /// Root proposals in push order.
        proposals: Vec<TraceProposal>,
        /// Retained entries.
        retained: Vec<RetainedEntry>,
    },
    /// One round.
    Round(TraceRound),
    /// First stop.
    Stop {
        /// Legacy stop name.
        stop: ExplorationStop,
        /// Rounds completed by the run.
        rounds_completed: u64,
    },
}

const fn mode_name(mode: ExplorationMode) -> &'static str {
    match mode {
        ExplorationMode::FaultSchedule => "fault-schedule",
        ExplorationMode::InputTree => "input-tree",
        ExplorationMode::Hybrid => "hybrid",
    }
}

/// Stable trace name of one branch slot.
pub(crate) fn slot_name(slot: BranchSlot) -> String {
    match slot {
        BranchSlot::Probe => "probe".to_owned(),
        BranchSlot::Variant(index) => format!("variant:{index}"),
        BranchSlot::Alternative(index) => format!("alternative:{index}"),
    }
}

impl TraceRecord {
    /// Header of one run.
    pub(crate) fn header(config: &ExplorerConfig, journal: Option<TraceJournal>) -> Self {
        Self::Header {
            schema: TRACE_SCHEMA.to_owned(),
            policy: config.frontier_policy,
            seed: config.seed,
            max_rounds: config.max_rounds,
            max_frontier: config.max_frontier,
            branch_factor: config.branch_factor,
            stale_round_limit: config.stale_round_limit,
            mode: mode_name(config.exploration_mode).to_owned(),
            journal,
        }
    }

    /// An executed round.
    pub(crate) fn executed(
        context: RoundContext,
        draws: RoundDraws,
        work: SelectionWork,
        selected: Option<&SelectedEntry>,
        observation: &RoundObservation,
        settlement: &Settlement,
        retained: Vec<RetainedEntry>,
    ) -> Self {
        let branches = observation
            .branches
            .iter()
            .map(|branch| TraceBranch {
                slot: slot_name(branch.slot),
                new_edges: branch.new_edges,
                snapshot: branch.snapshot_present,
                new_bugs: branch.findings.len(),
                proposals: branch
                    .retained
                    .as_ref()
                    .map_or_else(Vec::new, |retained| trace_proposals(&retained.proposals)),
            })
            .collect();
        Self::Round(TraceRound {
            round: context.round,
            mode: context.mode.as_str().to_owned(),
            outcome: OUTCOME_EXECUTED.to_owned(),
            draws: trace_draws(draws),
            selection: selected.map(|selected| TraceSelection {
                seq: selected.seq,
                explore: selected.explored,
            }),
            work: draws.draws.map(|_| work),
            branches,
            admitted: settlement.admitted.clone(),
            pruned: settlement.pruned.clone(),
            retained,
        })
    }

    /// A published round closed without execution.
    pub(crate) fn skipped(skipped: &SkippedRound, retained: Vec<RetainedEntry>) -> Self {
        Self::Round(TraceRound {
            round: skipped.context.round,
            mode: skipped.context.mode.as_str().to_owned(),
            outcome: OUTCOME_SKIPPED.to_owned(),
            draws: trace_draws(skipped.draws),
            selection: Some(TraceSelection {
                seq: skipped.seq,
                explore: skipped.explored,
            }),
            work: Some(skipped.work),
            branches: Vec::new(),
            admitted: Vec::new(),
            pruned: Vec::new(),
            retained,
        })
    }
}

/// Trace form of frontier-entry proposals, in push order.
pub(crate) fn trace_proposals(proposals: &[EntryProposal]) -> Vec<TraceProposal> {
    proposals
        .iter()
        .map(|proposal| TraceProposal {
            score_bits: proposal.score.to_bits(),
            marker: proposal.marker.is_some(),
        })
        .collect()
}

pub(crate) fn trace_draws(draws: RoundDraws) -> Option<TraceDraws> {
    draws.draws.map(|draws| TraceDraws {
        unit_bits: draws.unit.to_bits(),
        index: draws.index.and_then(|index| u64::try_from(index).ok()),
    })
}

/// Append-only trace writer; every record is flushed, the stop is synced.
pub(crate) struct FrontierTrace {
    writer: BufWriter<File>,
}

impl FrontierTrace {
    /// Opens (appending) `output_dir/frontier-trace.jsonl`.
    pub(crate) fn open(output_dir: &Path) -> io::Result<Self> {
        let path = output_dir.join(TRACE_FILE_NAME);
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self {
            writer: BufWriter::new(file),
        })
    }

    /// Writes and flushes one record.
    pub(crate) fn write(&mut self, record: &TraceRecord) -> io::Result<()> {
        serde_json::to_writer(&mut self.writer, record).map_err(io::Error::other)?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()
    }

    /// Writes the stop record and synchronizes the file.
    pub(crate) fn finish(&mut self, record: &TraceRecord) -> io::Result<()> {
        self.write(record)?;
        self.writer.get_ref().sync_all()
    }
}

/// A trace line that is not one schema record.
#[derive(Debug)]
pub struct TraceParseError {
    /// One-based line number.
    pub line: usize,
    /// Decoder failure.
    pub source: serde_json::Error,
}

impl std::fmt::Display for TraceParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "frontier trace line {}: {}",
            self.line, self.source
        )
    }
}

impl std::error::Error for TraceParseError {}

/// Parses every line of a trace; rejects unknown fields and record kinds.
pub(crate) fn parse_trace(text: &str) -> Result<Vec<TraceRecord>, TraceParseError> {
    text.lines()
        .enumerate()
        .map(|(index, line)| {
            serde_json::from_str(line).map_err(|source| TraceParseError {
                line: index.saturating_add(1),
                source,
            })
        })
        .collect()
}
