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
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TraceProposal {
    /// IEEE-754 bits of the score.
    pub(crate) score_bits: u64,
    /// Whether a marker created it.
    pub(crate) marker: bool,
    /// Exact assertion-hit provenance, when an assertion created it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) assertion: Option<crate::assertion_checkpoints::AssertionCheckpointMetadata>,
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
                    .iter()
                    .flat_map(|retained| trace_proposals(&retained.proposals))
                    .collect(),
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
            assertion: proposal.assertion.clone(),
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

/// Absolute bounds for read-only frontier-trace inspection.
pub const MAX_FRONTIER_TRACE_VIEW_RECORDS: usize = 10_000;
pub const MAX_FRONTIER_TRACE_VIEW_BYTES: usize = 8 * 1024 * 1024;

/// Retained product-decision positions, not simulation-time or causal edges.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrontierDecisionView {
    pub source_sequence: u64,
    pub kind: String,
    pub round: Option<u64>,
    pub selected_entry: Option<u64>,
    pub admitted: Vec<u64>,
    pub pruned: Vec<u64>,
    pub retained: Vec<u64>,
}

/// A read-only projection decoded with the producer's exact private schema.
/// This format does not retain simulation ticks or branch-parent identities.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrontierTraceSummary {
    pub records: u64,
    pub rounds: u64,
    pub stopped: bool,
    pub truncated: bool,
    pub root_present: bool,
    pub simulation_ticks_present: bool,
    pub branch_parentage_present: bool,
    pub decisions: Vec<FrontierDecisionView>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrontierTraceViewError {
    InputBound,
    InvalidRecord,
    InvalidShape,
}

impl std::fmt::Display for FrontierTraceViewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InputBound => "frontier trace exceeds inspection bounds",
            Self::InvalidRecord => "frontier trace record is malformed or unsupported",
            Self::InvalidShape => "frontier trace order, positions or accounting are invalid",
        })
    }
}

impl std::error::Error for FrontierTraceViewError {}

/// Decode retained decision facts without inferring VM ticks or fork edges.
/// A well-formed prefix without its stop is explicitly truncated; malformed
/// partial JSON is rejected, never silently discarded as a complete record.
pub fn inspect_frontier_trace(
    bytes: &[u8],
    max_records: usize,
) -> Result<FrontierTraceSummary, FrontierTraceViewError> {
    use FrontierTraceViewError::{InputBound, InvalidRecord, InvalidShape};
    if max_records == 0
        || max_records > MAX_FRONTIER_TRACE_VIEW_RECORDS
        || bytes.len() > MAX_FRONTIER_TRACE_VIEW_BYTES
    {
        return Err(InputBound);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| InvalidRecord)?;
    let mut lines = text.lines();
    let first = lines.next().ok_or(InvalidShape)?;
    let header: TraceRecord = serde_json::from_str(first).map_err(|_| InvalidRecord)?;
    let TraceRecord::Header {
        schema,
        max_rounds,
        max_frontier,
        branch_factor,
        mode,
        ..
    } = header
    else {
        return Err(InvalidShape);
    };
    if schema != TRACE_SCHEMA
        || max_frontier == 0
        || max_frontier > MAX_FRONTIER_TRACE_VIEW_RECORDS
        || branch_factor > MAX_FRONTIER_TRACE_VIEW_RECORDS
        || !matches!(mode.as_str(), "fault-schedule" | "input-tree" | "hybrid")
    {
        return Err(InvalidShape);
    }
    let mut view = FrontierTraceSummary {
        records: 1,
        rounds: 0,
        stopped: false,
        truncated: true,
        root_present: false,
        simulation_ticks_present: false,
        branch_parentage_present: false,
        decisions: vec![FrontierDecisionView {
            source_sequence: 0,
            kind: "header".into(),
            round: None,
            selected_entry: None,
            admitted: vec![],
            pruned: vec![],
            retained: vec![],
        }],
    };
    for line in lines {
        if view.records as usize >= max_records {
            return Err(InputBound);
        }
        if view.stopped {
            return Err(InvalidShape);
        }
        let record: TraceRecord = serde_json::from_str(line).map_err(|_| InvalidRecord)?;
        let sequence = view.records;
        let (kind, round, selected, admitted, pruned, retained) = match record {
            TraceRecord::Root {
                proposals,
                retained,
            } if sequence == 1 => {
                if proposals.len() > MAX_FRONTIER_TRACE_VIEW_RECORDS {
                    return Err(InputBound);
                }
                view.root_present = true;
                ("root", None, None, vec![], vec![], retained)
            }
            TraceRecord::Round(round) if view.root_present => {
                if round.round != view.rounds
                    || view.rounds >= max_rounds
                    || !matches!(round.outcome.as_str(), OUTCOME_EXECUTED | OUTCOME_SKIPPED)
                    || round.branches.len() > MAX_FRONTIER_TRACE_VIEW_RECORDS
                {
                    return Err(InvalidShape);
                }
                if round.selection.is_some_and(|selected| {
                    view.decisions
                        .last()
                        .is_none_or(|previous| !previous.retained.contains(&selected.seq))
                }) {
                    return Err(InvalidShape);
                }
                if round.admitted.len() > MAX_FRONTIER_TRACE_VIEW_RECORDS
                    || round.pruned.len() > MAX_FRONTIER_TRACE_VIEW_RECORDS
                    || round
                        .branches
                        .iter()
                        .any(|branch| branch.proposals.len() > MAX_FRONTIER_TRACE_VIEW_RECORDS)
                {
                    return Err(InputBound);
                }
                view.rounds += 1;
                (
                    "round",
                    Some(round.round),
                    round.selection.map(|s| s.seq),
                    round.admitted,
                    round.pruned,
                    round.retained,
                )
            }
            TraceRecord::Stop {
                rounds_completed, ..
            } if view.root_present => {
                if rounds_completed != view.rounds {
                    return Err(InvalidShape);
                }
                view.stopped = true;
                view.truncated = false;
                ("stop", None, None, vec![], vec![], vec![])
            }
            _ => return Err(InvalidShape),
        };
        if retained.len() > max_frontier {
            return Err(InputBound);
        }
        if retained.windows(2).any(|pair| pair[0].seq >= pair[1].seq)
            || retained
                .iter()
                .any(|entry| !f64::from_bits(entry.score_bits).is_finite())
        {
            return Err(InvalidShape);
        }
        let retained: Vec<_> = retained.iter().map(|entry| entry.seq).collect();
        view.decisions.push(FrontierDecisionView {
            source_sequence: sequence,
            kind: kind.into(),
            round,
            selected_entry: selected,
            admitted,
            pruned,
            retained,
        });
        view.records += 1;
    }
    Ok(view)
}

#[cfg(test)]
mod view_tests {
    use super::*;

    fn fixture() -> Vec<TraceRecord> {
        let config = ExplorerConfig {
            max_rounds: 1,
            max_frontier: 16,
            ..ExplorerConfig::default()
        };
        let score_bits = 1.0_f64.to_bits();
        vec![
            TraceRecord::header(&config, None),
            TraceRecord::Root {
                proposals: vec![TraceProposal {
                    score_bits,
                    marker: false,
                    assertion: None,
                }],
                retained: vec![RetainedEntry {
                    seq: 10,
                    times_selected: 0,
                    score_bits,
                }],
            },
            TraceRecord::Round(TraceRound {
                round: 0,
                mode: "fault-schedule".into(),
                outcome: OUTCOME_EXECUTED.into(),
                draws: Some(TraceDraws {
                    unit_bits: 0.5_f64.to_bits(),
                    index: None,
                }),
                selection: Some(TraceSelection {
                    seq: 10,
                    explore: false,
                }),
                work: Some(SelectionWork {
                    candidates_examined: 1,
                    eligible_candidates: 1,
                }),
                branches: vec![TraceBranch {
                    slot: "variant:0".into(),
                    new_edges: 1,
                    snapshot: true,
                    new_bugs: 0,
                    proposals: vec![TraceProposal {
                        score_bits,
                        marker: false,
                        assertion: None,
                    }],
                }],
                admitted: vec![11],
                pruned: vec![],
                retained: vec![
                    RetainedEntry {
                        seq: 10,
                        times_selected: 1,
                        score_bits,
                    },
                    RetainedEntry {
                        seq: 11,
                        times_selected: 0,
                        score_bits,
                    },
                ],
            }),
            TraceRecord::Stop {
                stop: ExplorationStop::Completed,
                rounds_completed: 1,
            },
        ]
    }

    fn encode(records: &[TraceRecord]) -> Vec<u8> {
        records
            .iter()
            .map(|record| serde_json::to_string(record).unwrap() + "\n")
            .collect::<String>()
            .into_bytes()
    }

    #[test]
    fn producer_records_project_exact_decision_positions_without_inferred_time_or_parentage() {
        let records = fixture();
        let bytes = encode(&records);
        let view = inspect_frontier_trace(&bytes, 4).unwrap();
        assert_eq!(view.records, 4);
        assert_eq!(view.rounds, 1);
        assert!(!view.truncated);
        assert!(view.stopped && view.root_present);
        assert!(!view.simulation_ticks_present && !view.branch_parentage_present);
        assert_eq!(view.decisions[2].source_sequence, 2);
        assert_eq!(view.decisions[2].round, Some(0));
        assert_eq!(view.decisions[2].selected_entry, Some(10));
        assert_eq!(view.decisions[2].admitted, [11]);
        assert_eq!(view.decisions[2].retained, [10, 11]);
        assert_eq!(inspect_frontier_trace(&bytes, 4).unwrap(), view);
        let prefix = inspect_frontier_trace(&encode(&records[..3]), 4).unwrap();
        assert!(prefix.truncated && !prefix.stopped);
    }

    #[test]
    fn malformed_reordered_missing_and_over_bound_trace_records_fail() {
        let records = fixture();
        assert_eq!(
            inspect_frontier_trace(&encode(&records), 3).unwrap_err(),
            FrontierTraceViewError::InputBound
        );
        let mut reordered = records.clone();
        reordered.swap(1, 2);
        assert_eq!(
            inspect_frontier_trace(&encode(&reordered), 4).unwrap_err(),
            FrontierTraceViewError::InvalidShape
        );
        let mut wrong = records.clone();
        if let TraceRecord::Round(round) = &mut wrong[2] {
            round.selection.as_mut().unwrap().seq = 999;
        }
        assert_eq!(
            inspect_frontier_trace(&encode(&wrong), 4).unwrap_err(),
            FrontierTraceViewError::InvalidShape
        );
        let mut wrong = records.clone();
        if let TraceRecord::Stop {
            rounds_completed, ..
        } = &mut wrong[3]
        {
            *rounds_completed = 2;
        }
        assert_eq!(
            inspect_frontier_trace(&encode(&wrong), 4).unwrap_err(),
            FrontierTraceViewError::InvalidShape
        );
        let mut partial = encode(&records[..3]);
        partial.extend_from_slice(b"{\"kind\":");
        assert_eq!(
            inspect_frontier_trace(&partial, 4).unwrap_err(),
            FrontierTraceViewError::InvalidRecord
        );
        assert_eq!(
            inspect_frontier_trace(&vec![b'x'; MAX_FRONTIER_TRACE_VIEW_BYTES + 1], 4).unwrap_err(),
            FrontierTraceViewError::InputBound
        );
    }
}
