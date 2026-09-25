//! Legacy-versus-Campaign parity corpus (VM-free) and the trace-pair checker.
//!
//! Every corpus drives the legacy backend and the Campaign backend (real
//! Campaign and Choregraph values, scratch journal) with the same script and
//! the same counted RNG, and compares every decision field after every round.

use super::super::trace::{FrontierTrace, TraceJournal, TraceRecord, TRACE_FILE_NAME};
use super::super::{EntryProposal, RoundBegin, RoundContext, RoundMode};
use super::drive::{
    drive_policy, open_backend, BranchScript, RngPosition, RoundEntropy, RoundScript, RunDecisions,
    RunScript,
};
use super::replay::{check_trace_bytes, check_trace_pair, write_receipt, ParityCheckError};
use super::report::{
    compare_runs, DecisionField, EvidenceQuestion, ParityReport, PolicySide, SideRole,
};
use crate::campaign_adapter::{AdapterError, BranchSlot, DegenerateConfig, ProductStop};
use crate::explorer::journal::{epoch_directory_name, JOURNAL_DIRECTORY_NAME};
use crate::explorer::{ExplorationMode, ExplorationStop, ExplorerConfig, FrontierPolicy};
use crate::marker_branching::MarkerFrontierMetadata;
use chaoscontrol_fault::engine::ChoiceRecord;
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::collections::VecDeque;
use std::path::Path;

const SEED: u64 = 0x5eed_2026;
const BRANCHES: usize = 2;
/// `stale_round_limit` that disables the plateau stop.
const NO_PLATEAU: u64 = 0;
/// Low bits `rand` 0.8 discards when drawing an `f64`.
const UNIT_DISCARDED_BITS: u32 = 11;
/// Largest `k` (`u = k * 2^-53`) that explores: `u < 0.1`.
const LAST_EXPLORE_K: u64 = 900_719_925_474_099;
/// Smallest `k` that exploits.
const FIRST_EXPLOIT_K: u64 = 900_719_925_474_100;
/// Largest `k`.
const MAX_K: u64 = (1 << 53) - 1;
/// A unit word that always exploits.
const EXPLOIT: u64 = MAX_K << UNIT_DISCARDED_BITS;
/// Rounds of the long single-entry decay run.
const LONG_RUN_ROUNDS: u64 = 200;
/// Options of an input-tree choice large enough to draw sampled alternatives.
const SAMPLED_CHOICE_OPTIONS: u32 = 20;

// ---------------------------------------------------------------------------
// Counted, scriptable entropy.

/// Forced words first, then a ChaCha8 stream; counts every word.
#[derive(Clone)]
struct ScriptedRng {
    forced: VecDeque<u64>,
    stream: ChaCha8Rng,
    words: u64,
}

impl RngCore for ScriptedRng {
    fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> u32::BITS) as u32
    }

    fn next_u64(&mut self) -> u64 {
        self.words += 1;
        self.forced
            .pop_front()
            .unwrap_or_else(|| self.stream.next_u64())
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        for chunk in dest.chunks_mut(8) {
            let word = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&word[..chunk.len()]);
        }
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}

struct LiveEntropy {
    rng: ScriptedRng,
}

impl LiveEntropy {
    fn new(seed: u64, forced: &[u64]) -> Self {
        Self {
            rng: ScriptedRng {
                forced: forced.iter().copied().collect(),
                stream: ChaCha8Rng::seed_from_u64(seed),
                words: 0,
            },
        }
    }
}

impl RoundEntropy for LiveEntropy {
    fn selection(&mut self, _round: u64) -> &mut dyn RngCore {
        &mut self.rng
    }

    fn selection_mismatch(&mut self, _round: u64) -> bool {
        false
    }

    fn product(&mut self) -> &mut dyn RngCore {
        &mut self.rng
    }

    fn position(&self) -> Option<RngPosition> {
        Some(RngPosition {
            words: self.rng.words,
            next_word: self.rng.clone().next_u64(),
        })
    }
}

const fn unit_word(k: u64) -> u64 {
    k << UNIT_DISCARDED_BITS
}

/// Word that makes `gen_range(0..len)` return `index` on its first draw.
fn index_word(index: u64, len: u64) -> u64 {
    u64::try_from((u128::from(index) << u64::BITS).div_ceil(u128::from(len))).unwrap()
}

// ---------------------------------------------------------------------------
// Scripts.

fn config(max_rounds: u64, max_frontier: usize) -> ExplorerConfig {
    ExplorerConfig {
        seed: SEED,
        max_rounds,
        max_frontier,
        branch_factor: BRANCHES,
        exploration_mode: ExplorationMode::FaultSchedule,
        stale_round_limit: NO_PLATEAU,
        ..ExplorerConfig::default()
    }
}

fn proposals(scores: &[f64]) -> Vec<EntryProposal> {
    scores
        .iter()
        .map(|score| EntryProposal {
            score: *score,
            marker: None,
        })
        .collect()
}

fn retained(slot: BranchSlot, scores: &[f64]) -> BranchScript {
    BranchScript {
        slot,
        new_edges: 1,
        snapshot: true,
        new_findings: 0,
        proposals: proposals(scores),
    }
}

fn variant(index: u32, scores: &[f64]) -> BranchScript {
    retained(BranchSlot::Variant(index), scores)
}

fn barren(index: u32) -> BranchScript {
    BranchScript {
        slot: BranchSlot::Variant(index),
        new_edges: 0,
        snapshot: true,
        new_findings: 0,
        proposals: Vec::new(),
    }
}

fn finding(index: u32) -> BranchScript {
    BranchScript {
        new_findings: 1,
        ..barren(index)
    }
}

fn round(branches: Vec<BranchScript>) -> RoundScript {
    RoundScript {
        branches,
        ..RoundScript::default()
    }
}

fn barren_round() -> RoundScript {
    round(vec![barren(0), barren(1)])
}

fn script(config: ExplorerConfig, root: &[f64], rounds: Vec<RoundScript>) -> RunScript {
    RunScript {
        config,
        root: proposals(root),
        rounds,
    }
}

fn up(value: f64) -> f64 {
    f64::from_bits(value.to_bits() + 1)
}

fn down(value: f64) -> f64 {
    f64::from_bits(value.to_bits() - 1)
}

fn marker(tick: u64) -> MarkerFrontierMetadata {
    MarkerFrontierMetadata {
        marker_identity: "parity.marker".to_owned(),
        owner: "parity".to_owned(),
        state_ref: Some("state".to_owned()),
        logical_position_ref: None,
        run_id: 0,
        observed_tick: tick,
    }
}

// ---------------------------------------------------------------------------
// Running and checking one corpus.

struct Corpus {
    name: &'static str,
    script: RunScript,
    forced: Vec<u64>,
}

struct CorpusRun {
    report: ParityReport,
    legacy: RunDecisions,
    campaign: RunDecisions,
}

/// Drives one policy. With `trace_dir`, the trace and (Campaign) journal go
/// there, as under an Explorer `output_dir`.
fn drive_live(policy: FrontierPolicy, corpus: &Corpus, trace_dir: Option<&Path>) -> RunDecisions {
    let scratch = tempfile::tempdir().unwrap();
    let output_dir = trace_dir.unwrap_or(scratch.path());
    let mut entropy = LiveEntropy::new(corpus.script.config.seed, &corpus.forced);
    let mut trace = trace_dir.map(|dir| {
        let mut trace = FrontierTrace::open(dir).unwrap();
        let config = ExplorerConfig {
            frontier_policy: policy,
            ..corpus.script.config.clone()
        };
        let journal = (policy == FrontierPolicy::CampaignCandidate).then_some(TraceJournal {
            epoch: 0,
            recovered_epochs: 0,
            closed_selections: 0,
        });
        trace.write(&TraceRecord::header(&config, journal)).unwrap();
        trace
    });
    drive_policy(
        policy,
        &corpus.script,
        &mut entropy,
        output_dir,
        trace.as_mut(),
    )
    .unwrap()
}

const LEGACY_LIVE: PolicySide = PolicySide {
    policy: FrontierPolicy::Legacy,
    role: SideRole::Live,
};
const CAMPAIGN_LIVE: PolicySide = PolicySide {
    policy: FrontierPolicy::CampaignCandidate,
    role: SideRole::Live,
};

fn run_corpus(corpus: &Corpus) -> CorpusRun {
    let legacy = drive_live(FrontierPolicy::Legacy, corpus, None);
    let campaign = drive_live(FrontierPolicy::CampaignCandidate, corpus, None);
    let report = compare_runs(corpus.name, LEGACY_LIVE, &legacy, CAMPAIGN_LIVE, &campaign);
    // The per-corpus report is emitted as JSON (visible with `--nocapture`).
    println!("{}", serde_json::to_string(&report).unwrap());
    CorpusRun {
        report,
        legacy,
        campaign,
    }
}

/// Runs a corpus and requires every decision field to agree after every round.
fn agreeing(corpus: &Corpus) -> CorpusRun {
    let run = run_corpus(corpus);
    if let Err(drift) = run.report.cutover_readiness() {
        panic!("{drift}\n{:#?}", run.report.decision_fields.drifts);
    }
    assert!(run.legacy.failure.is_none(), "{:?}", run.legacy.failure);
    assert_eq!(run.legacy, strip_work(&run.campaign, &run.legacy));
    if !run.legacy.rounds.is_empty() {
        assert!(run
            .report
            .fields_compared
            .contains(&DecisionField::RngPosition));
    }
    assert_eq!(run.report.time, EvidenceQuestion::NotMeasured);
    assert_eq!(run.report.memory, EvidenceQuestion::NotMeasured);
    run
}

/// `campaign` with `legacy`'s work, so full-value equality checks every
/// decision field while work stays a separate question.
fn strip_work(campaign: &RunDecisions, legacy: &RunDecisions) -> RunDecisions {
    let mut stripped = campaign.clone();
    for (round, legacy_round) in stripped.rounds.iter_mut().zip(&legacy.rounds) {
        round.work = legacy_round.work;
    }
    stripped
}

fn selections(run: &RunDecisions) -> Vec<Option<(u64, bool)>> {
    run.rounds
        .iter()
        .map(|round| {
            round
                .selection
                .map(|selection| (selection.seq, selection.explore))
        })
        .collect()
}

fn selected_seqs(run: &RunDecisions) -> Vec<u64> {
    run.rounds
        .iter()
        .filter_map(|round| round.selection.map(|selection| selection.seq))
        .collect()
}

// ---------------------------------------------------------------------------
// Positive corpora.

// r[verify chaoscontrol.campaign_policy.frontier_parity.valid]
// r[verify chaoscontrol.campaign_policy.ranks.valid]
#[test]
fn near_ties_agree() {
    let seven = 7.0;
    let rounds = vec![
        round(vec![variant(0, &[seven]), variant(1, &[down(seven)])]),
        round(vec![variant(0, &[up(seven)]), barren(1)]),
        round(vec![
            variant(0, &[down(down(seven))]),
            variant(1, &[up(up(seven))]),
        ]),
        barren_round(),
        round(vec![variant(0, &[up(seven), up(seven)]), barren(1)]),
        barren_round(),
    ];
    let run = agreeing(&Corpus {
        name: "near_ties",
        script: script(config(6, 4), &[seven, up(seven), down(seven)], rounds),
        forced: vec![EXPLOIT; 6],
    });
    // next_up(7) outranks 7; then 7 (seq 0) ties the equal child 7 (seq 3) and wins on seq.
    assert_eq!(selected_seqs(&run.legacy)[..2], [1, 0]);
    assert!(run
        .legacy
        .rounds
        .iter()
        .any(|round| !round.pruned.is_empty()));
}

// r[verify chaoscontrol.campaign_policy.frontier_parity.valid]
#[test]
fn equal_scores_agree() {
    let rounds = (0..6)
        .map(|_| round(vec![variant(0, &[5.0]), variant(1, &[5.0])]))
        .collect();
    let run = agreeing(&Corpus {
        name: "equal_scores",
        script: script(config(6, 3), &[5.0, 5.0, 5.0], rounds),
        forced: vec![EXPLOIT; 6],
    });
    // Equal effective scores resolve to the lowest sequence; equal children
    // are the newest of the equal lowest scores, so they are pruned at once.
    assert_eq!(selected_seqs(&run.legacy)[..3], [0, 1, 2]);
    for round in &run.legacy.rounds {
        assert_eq!(round.admitted, round.pruned);
    }
}

// r[verify chaoscontrol.campaign_policy.frontier_parity.valid]
// r[verify chaoscontrol.campaign_policy.ranks.valid]
#[test]
fn effective_score_ties_agree() {
    let rounds = (0..4).map(|_| barren_round()).collect();
    let run = agreeing(&Corpus {
        name: "effective_score_ties",
        script: script(config(4, 2), &[20.0, 10.0], rounds),
        forced: vec![EXPLOIT; 4],
    });
    // Round 1: 20 with n=1 ties 10 with n=0 at 10; the higher score wins.
    assert_eq!(selected_seqs(&run.legacy), [0, 0, 1, 0]);
}

// r[verify chaoscontrol.campaign_policy.frontier_parity.valid]
#[test]
fn subnormal_scores_agree() {
    let tiny = f64::from_bits(1);
    let rounds = vec![
        barren_round(),
        barren_round(),
        round(vec![variant(0, &[f64::from_bits(2)]), barren(1)]),
        barren_round(),
        round(vec![variant(0, &[f64::MIN_POSITIVE]), variant(1, &[tiny])]),
    ];
    let run = agreeing(&Corpus {
        name: "subnormals",
        script: script(config(5, 3), &[tiny, tiny], rounds),
        forced: vec![EXPLOIT; 5],
    });
    // tiny / 2 rounds to zero: after one selection seq 0 ranks at 0 and seq 1
    // wins; once both are zero the legacy scan keeps index 0 (seq 0).
    assert_eq!(selected_seqs(&run.legacy)[..3], [0, 1, 0]);
}

// r[verify chaoscontrol.campaign_policy.frontier_parity.valid]
#[test]
fn extreme_scores_agree() {
    let rounds = vec![
        round(vec![variant(0, &[f64::MAX]), barren(1)]),
        round(vec![variant(0, &[f64::MAX / 2.0]), variant(1, &[0.0])]),
        barren_round(),
        barren_round(),
    ];
    let run = agreeing(&Corpus {
        name: "extremes",
        script: script(config(4, 3), &[f64::MAX, down(f64::MAX), 0.0], rounds),
        forced: vec![EXPLOIT; 4],
    });
    assert_eq!(selected_seqs(&run.legacy)[0], 0);
    assert_eq!(run.legacy.rounds[0].pruned, [2]);
}

// r[verify chaoscontrol.campaign_policy.frontier_parity.valid]
#[test]
fn signed_zero_scores_agree() {
    let rounds = (0..4)
        .map(|_| round(vec![variant(0, &[-0.0]), variant(1, &[0.0])]))
        .collect();
    let run = agreeing(&Corpus {
        name: "signed_zeros",
        script: script(config(4, 3), &[0.0, -0.0, 0.0], rounds),
        forced: vec![EXPLOIT; 4],
    });
    // Every effective score is zero, so the legacy scan keeps index 0 (seq 0).
    assert_eq!(selected_seqs(&run.legacy), [0, 0, 0, 0]);
    let last = run.legacy.rounds.last().unwrap();
    assert!(last
        .retained
        .iter()
        .any(|entry| entry.score_bits == (-0.0_f64).to_bits()));
}

// r[verify chaoscontrol.campaign_policy.frontier_parity.valid]
#[test]
fn equal_lowest_scores_prune_alike_at_small_bounds() {
    let expected_first_prune: [&[u64]; 3] = [&[1, 2, 3], &[2, 3], &[3]];
    for (max_frontier, expected) in (1..=3).zip(expected_first_prune) {
        let rounds = vec![
            round(vec![variant(0, &[2.0, 2.0]), variant(1, &[2.0])]),
            round(vec![variant(0, &[3.0]), variant(1, &[2.0])]),
            round(vec![variant(0, &[2.0]), variant(1, &[2.0])]),
        ];
        let run = agreeing(&Corpus {
            name: "prune_equal_lowest",
            script: script(config(3, max_frontier), &[4.0], rounds),
            forced: vec![EXPLOIT; 3],
        });
        assert_eq!(
            run.legacy.rounds[0].pruned, expected,
            "max_frontier {max_frontier}"
        );
    }
}

// r[verify chaoscontrol.campaign_policy.frontier_parity.valid]
#[test]
fn a_child_pushed_then_pruned_in_the_same_round_agrees() {
    let rounds = vec![
        round(vec![variant(0, &[1.0]), barren(1)]),
        round(vec![variant(0, &[8.5]), variant(1, &[0.5])]),
    ];
    let run = agreeing(&Corpus {
        name: "pushed_then_pruned",
        script: script(config(2, 3), &[9.0, 8.0, 7.0], rounds),
        forced: vec![EXPLOIT; 2],
    });
    assert_eq!(run.legacy.rounds[0].admitted, [3]);
    assert_eq!(run.legacy.rounds[0].pruned, [3]);
    assert_eq!(run.legacy.rounds[1].pruned, [2, 5]);
}

// r[verify chaoscontrol.campaign_policy.frontier_parity.valid]
#[test]
fn the_selected_entry_at_the_retention_boundary_is_pruned_alike() {
    let rounds = vec![round(vec![variant(0, &[5.0]), barren(1)]), barren_round()];
    let run = agreeing(&Corpus {
        name: "selected_at_retention_boundary",
        script: script(config(2, 3), &[3.0, 2.0, 1.0], rounds),
        // Explore (k = 0) and draw index 2: the lowest entry, seq 2.
        forced: vec![unit_word(0), index_word(2, 3), EXPLOIT],
    });
    assert_eq!(
        run.legacy.rounds[0].selection.map(|s| (s.seq, s.explore)),
        Some((2, true))
    );
    assert_eq!(run.legacy.rounds[0].pruned, [2]);
}

// r[verify chaoscontrol.campaign_policy.frontier_parity.valid]
#[test]
fn root_proposals_beyond_the_frontier_bound_agree() {
    let rounds = vec![barren_round(), barren_round()];
    let run = agreeing(&Corpus {
        name: "root_beyond_max_frontier",
        script: script(config(2, 2), &[3.0, 2.0, 1.0], rounds),
        forced: vec![EXPLOIT; 2],
    });
    let root: Vec<u64> = run.legacy.root.iter().map(|entry| entry.seq).collect();
    assert_eq!(root, [0, 1]);
}

// r[verify chaoscontrol.campaign_policy.frontier_parity.valid]
#[test]
fn marker_branching_with_duplicate_metadata_agrees() {
    let duplicate = |score: f64| EntryProposal {
        score,
        marker: Some(marker(40)),
    };
    let marker_branch = |index: u32, scores: &[f64]| BranchScript {
        proposals: scores.iter().copied().map(duplicate).collect(),
        ..variant(index, &[])
    };
    let rounds = vec![
        round(vec![
            marker_branch(0, &[6.0, 6.0, 3.0]),
            marker_branch(1, &[6.0]),
        ]),
        round(vec![marker_branch(0, &[2.0, 2.0]), barren(1)]),
        round(vec![
            marker_branch(0, &[7.0]),
            marker_branch(1, &[7.0, 7.0]),
        ]),
        barren_round(),
    ];
    let corpus = Corpus {
        name: "marker_duplicates",
        script: RunScript {
            root: vec![duplicate(4.0), duplicate(4.0)],
            ..script(config(4, 4), &[], rounds)
        },
        forced: Vec::new(),
    };
    let run = agreeing(&corpus);
    assert_eq!(run.legacy.rounds[0].admitted, [2, 3, 4, 5]);
}

// r[verify chaoscontrol.campaign_policy.frontier_parity.valid]
// r[verify chaoscontrol.campaign_policy.entropy.valid]
#[test]
fn hybrid_rounds_with_input_tree_draws_agree() {
    const HYBRID_BRANCHES: usize = 3;
    let histories = vec![(
        0,
        vec![ChoiceRecord {
            sequence_id: 1,
            n_options: SAMPLED_CHOICE_OPTIONS,
            value: 3,
        }],
    )];
    let rounds: Vec<RoundScript> = (0..12_u32)
        .map(|index| {
            let score = f64::from(index % 5) + 1.5;
            if index % 2 == 0 {
                round(vec![
                    variant(0, &[score]),
                    barren(1),
                    variant(2, &[score + 1.0]),
                ])
            } else {
                RoundScript {
                    branches: vec![
                        retained(BranchSlot::Probe, &[score]),
                        retained(BranchSlot::Alternative(0), &[score + 0.25]),
                        BranchScript {
                            slot: BranchSlot::Alternative(1),
                            ..barren(0)
                        },
                    ],
                    choice_histories: histories.clone(),
                    signal: false,
                }
            }
        })
        .collect();
    let run = agreeing(&Corpus {
        name: "hybrid",
        script: script(
            ExplorerConfig {
                exploration_mode: ExplorationMode::Hybrid,
                branch_factor: HYBRID_BRANCHES,
                ..config(12, 4)
            },
            &[2.0],
            rounds,
        ),
        forced: Vec::new(),
    });
    let modes: Vec<RoundMode> = run.legacy.rounds.iter().map(|round| round.mode).collect();
    assert_eq!(modes[..2], [RoundMode::FaultSchedule, RoundMode::InputTree]);
    // An input-tree round draws its selection and then two sampled alternatives.
    let words: Vec<u64> = run
        .legacy
        .rounds
        .iter()
        .map(|round| round.rng_position.unwrap().words)
        .collect();
    assert!(words[1] - words[0] >= 3, "{words:?}");
}

// r[verify chaoscontrol.campaign_policy.entropy.valid]
#[test]
fn explore_boundary_draws_agree() {
    let rounds = (0..5).map(|_| barren_round()).collect();
    let forced = vec![
        unit_word(LAST_EXPLORE_K),
        index_word(1, 3),
        unit_word(FIRST_EXPLOIT_K),
        unit_word(0),
        index_word(2, 3),
        unit_word(MAX_K),
        unit_word(5),
        // Rejected by `gen_range(0..3)`'s acceptance zone, so a second word is drawn.
        u64::MAX,
        index_word(0, 3),
    ];
    let run = agreeing(&Corpus {
        name: "explore_boundary_k",
        script: script(config(5, 3), &[6.0, 4.0, 2.0], rounds),
        forced,
    });
    let explore: Vec<bool> = selections(&run.legacy)
        .into_iter()
        .map(|selection| selection.unwrap().1)
        .collect();
    assert_eq!(explore, [true, false, true, false, true]);
    assert_eq!(selected_seqs(&run.legacy)[0], 1);
    assert_eq!(selected_seqs(&run.legacy)[2], 2);
    assert_eq!(selected_seqs(&run.legacy)[4], 0);
    let words: Vec<u64> = run
        .legacy
        .rounds
        .iter()
        .map(|round| round.rng_position.unwrap().words)
        .collect();
    assert_eq!(words, [2, 3, 5, 6, 9]);
}

// r[verify chaoscontrol.campaign_policy.frontier_parity.valid]
#[test]
fn a_long_run_of_one_entry_decays_alike() {
    let rounds = (0..LONG_RUN_ROUNDS)
        .map(|_| round(vec![variant(0, &[0.5]), barren(1)]))
        .collect();
    let run = agreeing(&Corpus {
        name: "long_run_decay",
        script: script(config(LONG_RUN_ROUNDS, 1), &[1.0], rounds),
        forced: Vec::new(),
    });
    let last = run.legacy.rounds.last().unwrap();
    assert_eq!(last.retained.len(), 1);
    assert_eq!(last.retained[0].seq, 0);
    assert_eq!(u64::from(last.retained[0].times_selected), LONG_RUN_ROUNDS);
    assert_eq!(last.selection_use.selections, LONG_RUN_ROUNDS);
    assert_eq!(last.selection_use.budget_units, LONG_RUN_ROUNDS);
    // campaign.fd.evidence: equal fields, measured and different work. Campaign's
    // planner inspects every candidate record ever admitted; legacy scans the
    // retained entries only.
    assert!(run.report.decision_fields.equal);
    assert_eq!(
        run.report.semantic_work.answer,
        EvidenceQuestion::Measured { equal: false }
    );
    let examined = run.report.semantic_work.facts[0];
    assert!(examined.right_total > examined.left_total);
    let eligible = run.report.semantic_work.facts[1];
    assert!(eligible.equal);
}

/// The 30-round seeded script of the integration slice: ties, zeros, and pruning.
// r[verify chaoscontrol.campaign_policy.frontier_parity.valid]
#[test]
fn a_seeded_mixed_run_agrees() {
    let run = agreeing(&seeded_mixed());
    assert!(selections(&run.legacy)
        .iter()
        .any(|selection| selection.is_some_and(|(_, explore)| explore)));
    assert!(run
        .legacy
        .rounds
        .iter()
        .any(|round| !round.pruned.is_empty()));
}

fn seeded_mixed() -> Corpus {
    const SCORES: [f64; 7] = [3.0, 7.5, 7.5, 0.0, 12.25, 3.0, 9.0];
    const ROUNDS: u64 = 30;
    let rounds = (0..ROUNDS)
        .map(|index| {
            let pick = |offset: u64| SCORES[usize::try_from((index * 3 + offset) % 7).unwrap()];
            match index % 4 {
                0 => round(vec![variant(0, &[pick(0)]), barren(1)]),
                1 => round(vec![
                    variant(0, &[pick(1), pick(2)]),
                    variant(1, &[pick(3)]),
                ]),
                2 => barren_round(),
                _ => round(vec![variant(0, &[pick(4)]), variant(1, &[pick(5)])]),
            }
        })
        .collect();
    Corpus {
        name: "seeded_mixed",
        script: RunScript {
            config: ExplorerConfig {
                seed: 7,
                ..config(ROUNDS, 3)
            },
            ..script(config(ROUNDS, 3), &[5.0, 5.0], rounds)
        },
        forced: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Every stop class.

fn stop_corpus(
    name: &'static str,
    max_rounds: u64,
    stale_round_limit: u64,
    rounds: Vec<RoundScript>,
) -> Corpus {
    Corpus {
        name,
        script: script(
            ExplorerConfig {
                stale_round_limit,
                ..config(max_rounds, 3)
            },
            &[4.0, 3.0],
            rounds,
        ),
        forced: Vec::new(),
    }
}

fn productive_round(score: f64) -> RoundScript {
    round(vec![variant(0, &[score]), barren(1)])
}

// r[verify chaoscontrol.campaign_policy.frontier_parity.valid]
// r[verify chaoscontrol.campaign_policy.stop.valid]
#[test]
fn every_reachable_stop_class_agrees() {
    let cases = [
        (
            stop_corpus(
                "stop_plateau",
                8,
                3,
                vec![
                    productive_round(5.0),
                    productive_round(6.0),
                    barren_round(),
                    barren_round(),
                    barren_round(),
                    productive_round(7.0),
                    barren_round(),
                    barren_round(),
                ],
            ),
            ExplorationStop::CoveragePlateau,
            5,
        ),
        (
            stop_corpus(
                "stop_plateau_last_round",
                4,
                2,
                vec![
                    productive_round(5.0),
                    productive_round(6.0),
                    barren_round(),
                    barren_round(),
                ],
            ),
            ExplorationStop::CoveragePlateau,
            4,
        ),
        (
            stop_corpus(
                "stop_short_run_finding",
                5,
                NO_PLATEAU,
                vec![
                    productive_round(5.0),
                    productive_round(6.0),
                    round(vec![finding(0), barren(1)]),
                    productive_round(7.0),
                    productive_round(8.0),
                ],
            ),
            ExplorationStop::BugFound,
            3,
        ),
        (
            stop_corpus(
                "stop_signal",
                5,
                NO_PLATEAU,
                vec![
                    productive_round(5.0),
                    RoundScript {
                        signal: true,
                        ..productive_round(6.0)
                    },
                    productive_round(7.0),
                    productive_round(8.0),
                    productive_round(9.0),
                ],
            ),
            ExplorationStop::Interrupted,
            2,
        ),
        (
            // A finding does not stop a run of the short-run limit or longer.
            stop_corpus(
                "stop_completed",
                crate::explorer::round_stops::SHORT_RUN_ROUND_LIMIT,
                NO_PLATEAU,
                (0..crate::explorer::round_stops::SHORT_RUN_ROUND_LIMIT)
                    .map(|index| {
                        if index == 1 {
                            round(vec![finding(0), variant(1, &[2.0])])
                        } else {
                            productive_round(f64::from(u32::try_from(index).unwrap()))
                        }
                    })
                    .collect(),
            ),
            ExplorationStop::Completed,
            crate::explorer::round_stops::SHORT_RUN_ROUND_LIMIT,
        ),
    ];
    for (corpus, stop, rounds_completed) in cases {
        let run = agreeing(&corpus);
        assert_eq!(run.legacy.stop, Some(stop), "{}", corpus.name);
        assert_eq!(
            run.legacy.rounds_completed, rounds_completed,
            "{}",
            corpus.name
        );
    }
}

/// Frontier exhaustion is reachable only from a root with no proposals, which
/// the product never offers (`entry_proposals` yields one per snapshot).
/// Legacy then runs the live-controller clean-slate round that CC-D5 excludes;
/// Campaign stops before drawing. Both classify the stop as frontier
/// exhaustion, and the extra legacy round keeps cutover blocked.
// r[verify chaoscontrol.campaign_policy.frontier_parity.invalid]
#[test]
fn frontier_exhaustion_from_an_empty_root_blocks_cutover() {
    let run = run_corpus(&Corpus {
        name: "stop_frontier_exhausted",
        script: script(config(3, 2), &[], vec![barren_round(); 3]),
        forced: Vec::new(),
    });
    assert_eq!(run.legacy.stop, Some(ExplorationStop::FrontierExhausted));
    assert_eq!(run.campaign.stop, Some(ExplorationStop::FrontierExhausted));
    assert_eq!(run.legacy.rounds[0].selection, None);
    assert_eq!(run.legacy.rounds[0].draws, None);
    let drift = run.report.cutover_readiness().unwrap_err();
    assert_eq!(
        drift.fields,
        [DecisionField::RoundCount, DecisionField::RoundsCompleted]
    );
}

// ---------------------------------------------------------------------------
// Rejected degenerate configurations.

// r[verify chaoscontrol.campaign_policy.frontier_parity.invalid]
// r[verify chaoscontrol.campaign_policy.adapter.invalid]
#[test]
fn degenerate_configurations_are_rejected_and_block_cutover() {
    let cases = [
        (
            ExplorerConfig {
                max_frontier: 0,
                ..config(3, 3)
            },
            DegenerateConfig::ZeroMaxFrontier,
        ),
        (
            ExplorerConfig {
                rare_edge_weight: f64::NAN,
                ..config(3, 3)
            },
            DegenerateConfig::InvalidRareEdgeWeight,
        ),
        (
            ExplorerConfig {
                rare_edge_weight: -1.0,
                ..config(3, 3)
            },
            DegenerateConfig::InvalidRareEdgeWeight,
        ),
    ];
    for (config, expected) in cases {
        let scratch = tempfile::tempdir().unwrap();
        match open_backend(FrontierPolicy::CampaignCandidate, &config, scratch.path()) {
            Err(super::super::FrontierError::Adapter(AdapterError::DegenerateConfig(found))) => {
                assert_eq!(found, expected);
            }
            Err(other) => panic!("unexpected {other}"),
            Ok(_) => panic!("{expected:?} admitted"),
        }
        let run = run_corpus(&Corpus {
            name: "degenerate",
            script: script(config, &[4.0], vec![productive_round(5.0); 3]),
            forced: Vec::new(),
        });
        assert!(run.legacy.failure.is_none());
        assert!(run.campaign.failure.is_some());
        let drift = run.report.cutover_readiness().unwrap_err();
        assert!(
            drift.fields.contains(&DecisionField::BackendFailure),
            "{drift}"
        );
    }
}

// r[verify chaoscontrol.campaign_policy.stop.valid]
#[test]
fn zero_rounds_admit_no_selection() {
    let corpus = Corpus {
        name: "max_rounds_zero",
        script: script(config(0, 3), &[4.0], Vec::new()),
        forced: Vec::new(),
    };
    let run = agreeing(&corpus);
    assert_eq!(run.legacy.stop, Some(ExplorationStop::Completed));
    assert!(run.legacy.rounds.is_empty());
    // The Campaign backend itself refuses to plan a selection and draws nothing.
    let scratch = tempfile::tempdir().unwrap();
    let mut backend = open_backend(
        FrontierPolicy::CampaignCandidate,
        &corpus.script.config,
        scratch.path(),
    )
    .unwrap();
    let mut entropy = LiveEntropy::new(SEED, &[]);
    backend
        .admit_root(super::drive::root_admission(&proposals(&[4.0])))
        .unwrap();
    let context = RoundContext {
        round: 0,
        mode: RoundMode::FaultSchedule,
    };
    match backend.begin_round(entropy.selection(0), context).unwrap() {
        RoundBegin::Stop(stop) => assert_eq!(stop, ProductStop::CompletedWithoutSelection),
        RoundBegin::Planned(_) => panic!("a selection was planned with max_rounds 0"),
    }
    assert_eq!(entropy.position().unwrap().words, 0);
    assert_eq!(backend.selection_use().selections, 0);
}

// ---------------------------------------------------------------------------
// Report discipline.

// r[verify chaoscontrol.campaign_policy.frontier_parity.invalid]
#[test]
fn any_single_field_drift_blocks_cutover() {
    let run = agreeing(&seeded_mixed());
    let base = run.campaign.clone();
    type Mutation = fn(&mut RunDecisions);
    let mutations: [(DecisionField, Mutation); 10] = [
        (DecisionField::SelectedEntry, |run| {
            let selection = run.rounds[4].selection.as_mut().unwrap();
            selection.seq += 1;
        }),
        (DecisionField::ExploreFlag, |run| {
            let selection = run.rounds[4].selection.as_mut().unwrap();
            selection.explore = !selection.explore;
        }),
        (DecisionField::Pruned, |run| {
            run.rounds[5].pruned.push(u64::MAX)
        }),
        (DecisionField::Admitted, |run| {
            run.rounds[5].admitted.clear()
        }),
        (DecisionField::SelectionCounts, |run| {
            run.rounds[6].retained[0].times_selected += 1;
        }),
        (DecisionField::StopClass, |run| {
            run.stop = Some(ExplorationStop::CoveragePlateau);
        }),
        (DecisionField::SelectionUse, |run| {
            run.rounds[7].selection_use.budget_units += 1;
        }),
        (DecisionField::RngPosition, |run| {
            run.rounds[8].rng_position.as_mut().unwrap().words += 1;
        }),
        (DecisionField::ParentMoment, |run| {
            run.rounds[9].parent.as_mut().unwrap().depth += 1;
        }),
        (DecisionField::Draws, |run| {
            run.rounds[10].entropy_mismatch = true
        }),
    ];
    for (field, mutate) in mutations {
        let mut drifted = base.clone();
        mutate(&mut drifted);
        let report = compare_runs("drift", LEGACY_LIVE, &run.legacy, CAMPAIGN_LIVE, &drifted);
        let drift = report.cutover_readiness().unwrap_err();
        assert!(drift.fields.contains(&field), "{field:?}: {drift}");
        assert!(!report.decision_fields.equal);
    }
}

// campaign.fd.evidence.questions: equal work never stands in for equal fields,
// and different work alone does not block cutover.
#[test]
fn work_and_field_equality_are_separate_facts() {
    let run = agreeing(&seeded_mixed());
    let mut more_work = run.legacy.clone();
    for round in &mut more_work.rounds {
        if let Some(work) = round.work.as_mut() {
            work.candidates_examined += 1;
        }
    }
    let report = compare_runs("work", LEGACY_LIVE, &run.legacy, CAMPAIGN_LIVE, &more_work);
    assert!(report.decision_fields.equal);
    assert_eq!(
        report.semantic_work.answer,
        EvidenceQuestion::Measured { equal: false }
    );
    assert!(report.cutover_readiness().is_ok());
    let mut drifted = run.legacy.clone();
    drifted.rounds[3].pruned.push(u64::MAX);
    let report = compare_runs("fields", LEGACY_LIVE, &run.legacy, CAMPAIGN_LIVE, &drifted);
    assert!(!report.decision_fields.equal);
    assert_eq!(
        report.semantic_work.answer,
        EvidenceQuestion::Measured { equal: true }
    );
    assert!(report.cutover_readiness().is_err());
}

// ---------------------------------------------------------------------------
// Trace-pair checker.

/// Drives a corpus under both policies with traces, as two KVM runs would.
fn trace_pair(corpus: &Corpus) -> (tempfile::TempDir, tempfile::TempDir) {
    let legacy = tempfile::tempdir().unwrap();
    let campaign = tempfile::tempdir().unwrap();
    drive_live(FrontierPolicy::Legacy, corpus, Some(legacy.path()));
    drive_live(
        FrontierPolicy::CampaignCandidate,
        corpus,
        Some(campaign.path()),
    );
    (legacy, campaign)
}

fn stop_signal_corpus() -> Corpus {
    stop_corpus(
        "stop_signal",
        6,
        NO_PLATEAU,
        vec![
            productive_round(5.0),
            round(vec![variant(0, &[1.0, 9.0]), variant(1, &[0.5])]),
            RoundScript {
                signal: true,
                ..productive_round(6.0)
            },
            productive_round(7.0),
            productive_round(8.0),
            productive_round(9.0),
        ],
    )
}

// r[verify chaoscontrol.campaign_policy.frontier_parity.valid]
#[test]
fn driven_trace_pairs_agree_when_replayed_through_the_other_policy() {
    let corpora = [seeded_mixed(), stop_signal_corpus(), {
        let mut hybrid = seeded_mixed();
        hybrid.script.config.exploration_mode = ExplorationMode::Hybrid;
        hybrid
    }];
    for corpus in corpora {
        let (legacy, campaign) = trace_pair(&corpus);
        let legacy_path = legacy.path().join(TRACE_FILE_NAME);
        let campaign_path = campaign.path().join(TRACE_FILE_NAME);
        let receipt = check_trace_pair(&legacy_path, &campaign_path).unwrap();
        if let Err(drift) = receipt.cutover_readiness() {
            panic!("{}: {drift}", corpus.name);
        }
        assert!(receipt.cutover_ready && receipt.decision_fields_equal);
        assert_eq!(receipt.runs.len(), 2);
        assert_eq!(
            receipt.runs[0].replayed_through,
            FrontierPolicy::CampaignCandidate
        );
        assert_eq!(receipt.runs[1].replayed_through, FrontierPolicy::Legacy);
        let bytes = std::fs::read(&legacy_path).unwrap();
        assert_eq!(
            receipt.inputs[0].blake3,
            blake3::hash(&bytes).to_hex().to_string()
        );
        assert_eq!(receipt.time, EvidenceQuestion::NotMeasured);
        assert!(receipt
            .non_claims
            .iter()
            .any(|claim| claim.starts_with("live coverage drift")));
        // Decisions agree while the planners' measured work differs.
        assert_eq!(
            receipt.semantic_work,
            EvidenceQuestion::Measured { equal: false }
        );
        // Only the Campaign run journals: genesis, two segments per round, a stop.
        assert!(!legacy.path().join(JOURNAL_DIRECTORY_NAME).exists());
        let segments = std::fs::read_dir(
            campaign
                .path()
                .join(JOURNAL_DIRECTORY_NAME)
                .join(epoch_directory_name(0)),
        )
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("seg-")
        })
        .count();
        let rounds = usize::try_from(receipt.runs[1].report.rounds_compared).unwrap();
        assert_eq!(segments, 1 + 2 * rounds + 1, "{}", corpus.name);
    }
}

const FIXTURE_LEGACY: &[u8] =
    include_bytes!("../../../../tests/fixtures/frontier-parity/legacy.valid.jsonl");
const FIXTURE_CAMPAIGN: &[u8] =
    include_bytes!("../../../../tests/fixtures/frontier-parity/campaign.valid.jsonl");
const FIXTURE_FLIPPED_SELECTION: &[u8] = include_bytes!(
    "../../../../tests/fixtures/frontier-parity/legacy.flipped-selection.invalid.jsonl"
);
const FIXTURE_FLIPPED_PRUNE: &[u8] =
    include_bytes!("../../../../tests/fixtures/frontier-parity/legacy.flipped-prune.invalid.jsonl");
const FIXTURE_FLIPPED_STOP: &[u8] = include_bytes!(
    "../../../../tests/fixtures/frontier-parity/campaign.flipped-stop.invalid.jsonl"
);

fn check_fixtures(
    legacy: &[u8],
    campaign: &[u8],
) -> Result<super::replay::FrontierParityReceipt, ParityCheckError> {
    check_trace_bytes(
        (Path::new("legacy.jsonl"), legacy),
        (Path::new("campaign.jsonl"), campaign),
    )
}

// r[verify chaoscontrol.campaign_policy.frontier_parity.valid]
#[test]
fn the_committed_agreeing_fixture_pair_is_cutover_ready() {
    let receipt = check_fixtures(FIXTURE_LEGACY, FIXTURE_CAMPAIGN).unwrap();
    receipt.cutover_readiness().unwrap();
    assert!(receipt
        .runs
        .iter()
        .all(|run| run.report.rounds_compared > 0));
}

// r[verify chaoscontrol.campaign_policy.frontier_parity.invalid]
#[test]
fn committed_fixtures_with_a_flipped_decision_block_cutover() {
    let cases = [
        (
            FIXTURE_FLIPPED_SELECTION,
            FIXTURE_CAMPAIGN,
            0,
            DecisionField::SelectedEntry,
        ),
        (
            FIXTURE_FLIPPED_PRUNE,
            FIXTURE_CAMPAIGN,
            0,
            DecisionField::Pruned,
        ),
        (
            FIXTURE_LEGACY,
            FIXTURE_FLIPPED_STOP,
            1,
            DecisionField::StopClass,
        ),
    ];
    for (legacy, campaign, run, field) in cases {
        let receipt = check_fixtures(legacy, campaign).unwrap();
        assert!(!receipt.cutover_ready);
        let drift = receipt.runs[run].report.cutover_readiness().unwrap_err();
        assert!(drift.fields.contains(&field), "{field:?}: {drift}");
        assert!(receipt.cutover_readiness().is_err());
        // The untouched run still agrees.
        assert!(receipt.runs[1 - run].report.decision_fields.equal);
    }
}

#[test]
fn malformed_trace_pairs_are_typed_errors() {
    let legacy = std::str::from_utf8(FIXTURE_LEGACY).unwrap();
    let campaign = std::str::from_utf8(FIXTURE_CAMPAIGN).unwrap();
    let check =
        |legacy: &str, campaign: &str| check_fixtures(legacy.as_bytes(), campaign.as_bytes()).err();
    // Roles swapped: the legacy slot holds a Campaign header.
    assert!(matches!(
        check(campaign, campaign),
        Some(ParityCheckError::PolicyRole { .. })
    ));
    // Different seeds.
    let reseeded = campaign.replacen("\"seed\":", "\"seed\":1", 1);
    assert!(matches!(
        check(legacy, &reseeded),
        Some(ParityCheckError::ConfigMismatch { field: "seed" })
    ));
    // A resumed run appends a second header.
    let header = legacy.lines().next().unwrap();
    let resumed = format!("{legacy}{header}\n");
    assert!(matches!(
        check(&resumed, campaign),
        Some(ParityCheckError::Shape { .. })
    ));
    // Unknown fields are rejected by the trace schema.
    let unknown = legacy.replacen("\"retained\"", "\"retrained\"", 1);
    assert!(matches!(
        check(&unknown, campaign),
        Some(ParityCheckError::Parse { .. })
    ));
    // No stop record.
    let unstopped: String = legacy
        .lines()
        .filter(|line| !line.contains("\"kind\":\"stop\""))
        .map(|line| format!("{line}\n"))
        .collect();
    assert!(matches!(
        check(&unstopped, campaign),
        Some(ParityCheckError::Shape { .. })
    ));
    // Admitted entries that do not match the recorded proposals.
    let inconsistent = legacy.replacen("\"admitted\":[", "\"admitted\":[999,", 1);
    assert!(matches!(
        check(&inconsistent, campaign),
        Some(ParityCheckError::InconsistentRound { .. })
    ));
}

#[test]
fn a_receipt_is_written_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("frontier-parity.json");
    let receipt = check_fixtures(FIXTURE_LEGACY, FIXTURE_CAMPAIGN).unwrap();
    write_receipt(&path, &receipt).unwrap();
    let written: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(written["schema"], super::replay::PARITY_RECEIPT_SCHEMA);
    assert_eq!(written["cutover_ready"], true);
    assert_eq!(written["time"]["status"], "not_measured");
    assert_eq!(
        written["runs"][0]["report"]["semantic_work"]["answer"]["status"],
        "measured"
    );
    assert!(matches!(
        write_receipt(&path, &receipt),
        Err(ParityCheckError::ReceiptWrite { .. })
    ));
}
