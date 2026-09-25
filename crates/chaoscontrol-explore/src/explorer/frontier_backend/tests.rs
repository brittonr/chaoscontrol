//! VM-free tests of the frontier backends and the round protocol.
//!
//! The Campaign backend runs against real Campaign and Choregraph values and a
//! real journal on a temporary directory whose publication effects are scripted
//! (`ScriptedIo`), so every durability class can be injected at the selection
//! or settlement segment.

use super::campaign::CheckpointBinding;
use super::records::ReplayFailure;
use super::trace::TraceRecord;
use super::*;
use crate::campaign_adapter::{AdapterIdentity, AdapterReason, BranchSlot, DegenerateConfig};
use crate::checkpoint::CampaignJournalBinding;
use crate::explorer::journal::{
    CampaignJournal, JournalError, JournalRecord, PublicationIo, PublicationPhase, RecordKind,
    SystemPublicationIo, JOURNAL_DIRECTORY_NAME,
};
use crate::explorer::{ExplorationMode, ExplorerConfig, FrontierPolicy};
use campaign_core::{CandidateStatus, StopReason, WaitReason};
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::cell::RefCell;
use std::io;
use std::path::Path;
use std::rc::Rc;

const SEED: u64 = 7;
const ROUNDS: u64 = 30;
const MAX_FRONTIER: usize = 3;
const BRANCHES: usize = 2;
const VM_COUNT: usize = 2;
const NETWORK_SEED: u64 = 42;
const BOOTSTRAP_TICK: u64 = 11;
const ROOT_SCORES: [f64; 2] = [5.0, 5.0];
const BRANCH_TICKS: u64 = 100;
const BASE_SET: [u8; 32] = [4; 32];

fn dummy_snapshot() -> SimulationSnapshot {
    let engine = chaoscontrol_fault::engine::FaultEngine::new(
        chaoscontrol_fault::engine::EngineConfig::default(),
    );
    SimulationSnapshot {
        tick: 0,
        vm_snapshots: Vec::new(),
        network_state: chaoscontrol_vmm::controller::NetworkFabric::new(VM_COUNT, NETWORK_SEED),
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

fn config(policy: FrontierPolicy, output: &Path) -> ExplorerConfig {
    ExplorerConfig {
        seed: SEED,
        max_rounds: ROUNDS,
        max_frontier: MAX_FRONTIER,
        branch_factor: BRANCHES,
        exploration_mode: ExplorationMode::FaultSchedule,
        output_dir: Some(output.display().to_string()),
        frontier_policy: policy,
        ..ExplorerConfig::default()
    }
}

fn digests(tick: u64) -> Option<BranchDigests> {
    Some(BranchDigests {
        schedule: [1; 32],
        variant: [2; 32],
        coverage: [3; 32],
        oracle: [5; 32],
        total_ticks: tick,
    })
}

fn proposal(score: f64) -> EntryProposal {
    EntryProposal {
        score,
        marker: None,
    }
}

fn root(snapshot: Option<SimulationSnapshot>) -> RootAdmission {
    RootAdmission {
        snapshot,
        coverage: CoverageBitmap::new(),
        schedule: FaultSchedule::new(),
        proposals: ROOT_SCORES.iter().copied().map(proposal).collect(),
        digests: digests(BOOTSTRAP_TICK),
        base_set_id: BASE_SET,
    }
}

/// One scripted branch: `None` = no new edges; `Some(scores)` = retained.
type BranchScript = Option<Vec<f64>>;

fn observation(depth: u32, branches: &[BranchScript]) -> RoundObservation {
    RoundObservation {
        branches: branches
            .iter()
            .enumerate()
            .map(|(index, branch)| BranchOutcome {
                slot: BranchSlot::Variant(u32::try_from(index).unwrap()),
                new_edges: usize::from(branch.is_some()),
                snapshot_present: true,
                findings: Vec::new(),
                retained: branch.as_ref().map(|scores| RetainedBranch {
                    snapshot: dummy_snapshot(),
                    coverage: CoverageBitmap::new(),
                    schedule: FaultSchedule::new(),
                    depth,
                    proposals: scores.iter().copied().map(proposal).collect(),
                    digests: digests(BRANCH_TICKS),
                }),
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Scripted publication effects.

#[derive(Clone, Copy, Debug)]
struct Fault {
    phase: PublicationPhase,
    kind: io::ErrorKind,
    remaining: u32,
}

#[derive(Clone, Default)]
struct ScriptedIo {
    inner: SystemPublicationIo,
    faults: Rc<RefCell<Vec<Fault>>>,
    log: Rc<RefCell<Vec<String>>>,
    publishing: Rc<RefCell<Option<std::path::PathBuf>>>,
}

impl ScriptedIo {
    fn arm(&self, phase: PublicationPhase, kind: io::ErrorKind, times: u32) {
        self.faults.borrow_mut().push(Fault {
            phase,
            kind,
            remaining: times,
        });
    }

    fn trip(&self, phase: PublicationPhase) -> io::Result<()> {
        let mut faults = self.faults.borrow_mut();
        if let Some(fault) = faults
            .iter_mut()
            .find(|fault| fault.phase == phase && fault.remaining > 0)
        {
            fault.remaining -= 1;
            return Err(io::Error::new(fault.kind, "scripted fault"));
        }
        Ok(())
    }

    fn events(&self) -> Vec<String> {
        self.log.borrow().clone()
    }
}

impl PublicationIo for ScriptedIo {
    type Stage = tempfile::NamedTempFile;

    fn create_stage(&mut self, directory: &Path) -> io::Result<Self::Stage> {
        self.trip(PublicationPhase::CreateStage)?;
        self.inner.create_stage(directory)
    }

    fn write_payload(&mut self, stage: &mut Self::Stage, bytes: &[u8]) -> io::Result<()> {
        self.trip(PublicationPhase::WritePayload)?;
        self.inner.write_payload(stage, bytes)
    }

    fn sync_payload(&mut self, stage: &mut Self::Stage) -> io::Result<()> {
        self.trip(PublicationPhase::SyncPayload)?;
        self.inner.sync_payload(stage)
    }

    fn persist_no_replace(&mut self, stage: Self::Stage, destination: &Path) -> io::Result<()> {
        self.trip(PublicationPhase::PersistNoReplace)?;
        self.inner.persist_no_replace(stage, destination)?;
        *self.publishing.borrow_mut() = Some(destination.to_path_buf());
        Ok(())
    }

    fn create_directory(&mut self, path: &Path) -> io::Result<()> {
        self.inner.create_directory(path)
    }

    fn sync_directory(&mut self, path: &Path) -> io::Result<()> {
        let published = self.publishing.borrow_mut().take();
        if published.is_some() {
            self.trip(PublicationPhase::SyncDirectory)?;
        }
        self.inner.sync_directory(path)?;
        if let Some(segment) = published {
            let name = segment.file_name().unwrap().to_string_lossy().into_owned();
            self.log.borrow_mut().push(format!("commit {name}"));
        }
        Ok(())
    }
}

fn open_campaign(dir: &Path, io: ScriptedIo) -> CampaignFrontier<ScriptedIo> {
    let config = config(FrontierPolicy::CampaignCandidate, dir);
    let mut frontier = CampaignFrontier::open_with_io(&config, dir, None, io).unwrap();
    frontier.admit_root(root(Some(dummy_snapshot()))).unwrap();
    frontier
}

fn context(round: u64) -> RoundContext {
    RoundContext {
        round,
        mode: RoundMode::FaultSchedule,
    }
}

fn planned(frontier: &mut dyn FrontierBackend, rng: &mut ChaCha8Rng, round: u64) -> PlannedRound {
    match frontier.begin_round(rng, context(round)).unwrap() {
        RoundBegin::Planned(planned) => planned,
        RoundBegin::Stop(stop) => panic!("unexpected stop {stop:?}"),
    }
}

fn executable(authorization: Authorization) -> AuthorizedRound {
    match authorization {
        Authorization::Execute(round) => *round,
        Authorization::Skipped(skipped) => panic!("unexpected skip {skipped:?}"),
    }
}

fn segment_count(dir: &Path, epoch: u32) -> usize {
    let epoch_dir = dir
        .join(JOURNAL_DIRECTORY_NAME)
        .join(crate::explorer::journal::epoch_directory_name(epoch));
    std::fs::read_dir(epoch_dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("seg-"))
        .count()
}

// ---------------------------------------------------------------------------
// Publication fence.

// r[verify chaoscontrol.campaign_policy.publication_fence.valid]
#[test]
fn the_selection_segment_commits_before_the_round_can_execute() {
    let dir = tempfile::tempdir().unwrap();
    let io = ScriptedIo::default();
    let mut frontier = open_campaign(dir.path(), io.clone());
    let mut rng = ChaCha8Rng::seed_from_u64(SEED);
    let genesis_segments = segment_count(dir.path(), 0);
    let plan = planned(&mut frontier, &mut rng, 0);
    // Planning publishes nothing.
    assert_eq!(segment_count(dir.path(), 0), genesis_segments);
    let before = io.events().len();
    let round = executable(frontier.authorize(plan).unwrap());
    let committed: Vec<String> = io.events()[before..].to_vec();
    assert_eq!(committed.len(), 1, "exactly the selection segment commits");
    assert_eq!(segment_count(dir.path(), 0), genesis_segments + 1);
    // Only now may the shell restore the parent snapshot and execute.
    let selected = round.selected().expect("campaign rounds always select");
    io.log.borrow_mut().push("execute".to_owned());
    assert_eq!(
        io.events().last().map(String::as_str),
        Some("execute"),
        "execution follows the committed selection segment"
    );
    assert!(selected.seq < u64::try_from(ROOT_SCORES.len()).unwrap());
    frontier
        .finish_round(round, &observation(1, &[None, None]))
        .unwrap();
    assert_eq!(segment_count(dir.path(), 0), genesis_segments + 2);
}

// r[verify chaoscontrol.campaign_policy.publication_fence.invalid]
#[test]
fn every_failed_selection_publication_blocks_the_round() {
    let cases = [
        // Not committed: nothing visible, pending state dropped, backend halted.
        (PublicationPhase::WritePayload, io::ErrorKind::Other, false),
        (PublicationPhase::SyncPayload, io::ErrorKind::Other, false),
        // Durability unknown: journal poisoned.
        (PublicationPhase::SyncDirectory, io::ErrorKind::Other, true),
        (
            PublicationPhase::PersistNoReplace,
            io::ErrorKind::Other,
            true,
        ),
    ];
    for (phase, kind, poisons) in cases {
        let dir = tempfile::tempdir().unwrap();
        let io = ScriptedIo::default();
        let mut frontier = open_campaign(dir.path(), io.clone());
        let mut rng = ChaCha8Rng::seed_from_u64(SEED);
        let plan = planned(&mut frontier, &mut rng, 0);
        let state_before = frontier.state().unwrap().clone();
        io.arm(phase, kind, 1);
        let error = match frontier.authorize(plan) {
            Err(error) => error,
            Ok(_) => panic!("{phase:?} must not authorize execution"),
        };
        assert!(
            matches!(error, FrontierError::Journal(_)),
            "{phase:?}: {error}"
        );
        assert_eq!(frontier.journal().is_poisoned(), poisons, "{phase:?}");
        // The pending selection was dropped; no round can start again.
        assert_eq!(frontier.state().unwrap(), &state_before, "{phase:?}");
        assert!(matches!(
            frontier.begin_round(&mut rng, context(1)),
            Err(FrontierError::Halted)
        ));
    }
}

// r[verify chaoscontrol.campaign_policy.publication_fence.invalid]
#[test]
fn uncertain_durability_poisons_the_journal_and_stops_every_later_step() {
    let dir = tempfile::tempdir().unwrap();
    let io = ScriptedIo::default();
    let mut frontier = open_campaign(dir.path(), io.clone());
    let mut rng = ChaCha8Rng::seed_from_u64(SEED);
    let plan = planned(&mut frontier, &mut rng, 0);
    io.arm(PublicationPhase::SyncDirectory, io::ErrorKind::Other, 1);
    let error = frontier
        .authorize(plan)
        .err()
        .expect("uncertain durability");
    assert!(matches!(
        error,
        FrontierError::Journal(JournalError::DurabilityUnknown { .. })
    ));
    assert!(frontier.journal().is_poisoned());
    assert!(matches!(
        frontier.record_stop(ProductStop::Interrupted),
        Err(FrontierError::Halted)
    ));
    assert!(matches!(
        frontier
            .journal_mut()
            .append(&[JournalRecord::new(RecordKind::new(1), vec![0])]),
        Err(JournalError::Poisoned)
    ));
}

// r[verify chaoscontrol.campaign_policy.publication_fence.invalid]
#[test]
fn a_stale_selection_sequence_replans_once_with_the_same_draws() {
    let dir = tempfile::tempdir().unwrap();
    let io = ScriptedIo::default();
    let mut frontier = open_campaign(dir.path(), io.clone());
    let mut rng = ChaCha8Rng::seed_from_u64(SEED);
    let mut expected_rng = rng.clone();
    let plan = planned(&mut frontier, &mut rng, 0);
    let draws = plan.draws();
    crate::frontier::draw_selection(&mut expected_rng, ROOT_SCORES.len());
    io.arm(
        PublicationPhase::PersistNoReplace,
        io::ErrorKind::AlreadyExists,
        1,
    );
    let round = executable(frontier.authorize(plan).unwrap());
    assert_eq!(round.draws(), draws, "the replan reuses the recorded draws");
    assert_eq!(
        rng.next_u64(),
        expected_rng.next_u64(),
        "the replan never redraws"
    );
    assert_eq!(frontier.state().unwrap().outstanding.len(), 1);
    frontier
        .finish_round(round, &observation(1, &[None, None]))
        .unwrap();

    // A second collision on the same round is a typed stale-branch stop.
    let plan = planned(&mut frontier, &mut rng, 1);
    io.arm(
        PublicationPhase::PersistNoReplace,
        io::ErrorKind::AlreadyExists,
        2,
    );
    assert!(matches!(
        frontier.authorize(plan),
        Err(FrontierError::StaleControlBranch)
    ));
    assert!(matches!(
        frontier.begin_round(&mut rng, context(2)),
        Err(FrontierError::Halted)
    ));
}

#[test]
fn a_foreign_segment_at_the_selection_sequence_is_a_stale_branch() {
    let dir = tempfile::tempdir().unwrap();
    let mut frontier = open_campaign(dir.path(), ScriptedIo::default());
    let mut rng = ChaCha8Rng::seed_from_u64(SEED);
    let plan = planned(&mut frontier, &mut rng, 0);
    // Another writer's segment already occupies the next sequence.
    let foreign = crate::explorer::journal::encode_segment(
        0,
        frontier.journal().next_seq().unwrap(),
        frontier.last_segment().unwrap().digest(),
        &[JournalRecord::new(RecordKind::new(1), vec![1])],
    )
    .unwrap();
    let path = dir
        .path()
        .join(JOURNAL_DIRECTORY_NAME)
        .join(crate::explorer::journal::epoch_directory_name(0))
        .join(crate::explorer::journal::segment_file_name(
            frontier.journal().next_seq().unwrap(),
        ));
    std::fs::write(&path, foreign.bytes()).unwrap();
    assert!(matches!(
        frontier.authorize(plan),
        Err(FrontierError::Replay(_) | FrontierError::StaleControlBranch)
    ));
    assert_eq!(std::fs::read(&path).unwrap(), foreign.bytes());
}

// ---------------------------------------------------------------------------
// Waits, stops, degenerate configurations, eligibility.

// r[verify chaoscontrol.campaign_policy.stop.wait]
#[test]
fn an_outstanding_selection_is_a_wait_never_exhaustion() {
    let dir = tempfile::tempdir().unwrap();
    let mut frontier = open_campaign(dir.path(), ScriptedIo::default());
    let mut rng = ChaCha8Rng::seed_from_u64(SEED);
    let plan = planned(&mut frontier, &mut rng, 0);
    let _outstanding = executable(frontier.authorize(plan).unwrap());
    match frontier.begin_round(&mut rng, context(1)) {
        Err(FrontierError::UnexpectedWait(reason)) => {
            assert_eq!(reason, WaitReason::AwaitingCapacity);
        }
        Err(other) => panic!("wait mapped to {other}"),
        Ok(RoundBegin::Stop(stop)) => panic!("wait reported as stop {stop:?}"),
        Ok(RoundBegin::Planned(_)) => panic!("second selection while one is outstanding"),
    }
}

// r[verify chaoscontrol.campaign_policy.adapter.invalid]
#[test]
fn degenerate_configurations_are_rejected_before_the_journal_opens() {
    let dir = tempfile::tempdir().unwrap();
    let base = config(FrontierPolicy::CampaignCandidate, dir.path());
    let cases = [
        (
            ExplorerConfig {
                max_frontier: 0,
                ..base.clone()
            },
            DegenerateConfig::ZeroMaxFrontier,
        ),
        (
            ExplorerConfig {
                rare_edge_weight: f64::NAN,
                ..base.clone()
            },
            DegenerateConfig::InvalidRareEdgeWeight,
        ),
        (
            ExplorerConfig {
                rare_edge_weight: -1.0,
                ..base.clone()
            },
            DegenerateConfig::InvalidRareEdgeWeight,
        ),
        (
            ExplorerConfig {
                rare_edge_weight: f64::INFINITY,
                ..base.clone()
            },
            DegenerateConfig::InvalidRareEdgeWeight,
        ),
    ];
    for (config, expected) in cases {
        match CampaignFrontier::open_with_io(&config, dir.path(), None, ScriptedIo::default()) {
            Err(FrontierError::Adapter(
                crate::campaign_adapter::AdapterError::DegenerateConfig(found),
            )) => assert_eq!(found, expected),
            Err(other) => panic!("unexpected {other}"),
            Ok(_) => panic!("{expected:?} admitted"),
        }
        assert!(!dir.path().join(JOURNAL_DIRECTORY_NAME).exists());
    }
    // A bootstrap without a restorable snapshot has no Campaign root.
    let mut frontier =
        CampaignFrontier::open_with_io(&base, dir.path(), None, ScriptedIo::default()).unwrap();
    assert!(matches!(
        frontier.admit_root(root(None)),
        Err(FrontierError::Adapter(
            crate::campaign_adapter::AdapterError::DegenerateConfig(
                DegenerateConfig::MissingRestorableRoot
            )
        ))
    ));
}

// r[verify chaoscontrol.campaign_policy.snapshot_eligibility.invalid]
#[test]
fn an_unrestorable_snapshot_after_publication_closes_and_retires() {
    let dir = tempfile::tempdir().unwrap();
    let mut frontier = open_campaign(dir.path(), ScriptedIo::default());
    let mut rng = ChaCha8Rng::seed_from_u64(SEED);
    let plan = planned(&mut frontier, &mut rng, 0);
    frontier.forget_snapshots();
    let skipped = match frontier.authorize(plan).unwrap() {
        Authorization::Skipped(skipped) => skipped,
        Authorization::Execute(_) => panic!("executed without a restorable snapshot"),
    };
    let state = frontier.state().unwrap();
    assert!(state.outstanding.is_empty());
    let retired = state
        .candidates
        .iter()
        .filter(|candidate| matches!(candidate.status, CandidateStatus::Retired { .. }))
        .count();
    assert_eq!(retired, 1);
    assert_eq!(frontier.len(), ROOT_SCORES.len() - 1);
    assert!(frontier
        .retained()
        .iter()
        .all(|entry| entry.seq != skipped.seq));
    // Before the next plan, the remaining unrestorable candidate retires too and
    // the empty frontier stops without drawing.
    let mut expected_rng = rng.clone();
    assert!(matches!(
        frontier.begin_round(&mut rng, context(1)).unwrap(),
        RoundBegin::Stop(ProductStop::FrontierExhausted)
    ));
    assert_eq!(rng.next_u64(), expected_rng.next_u64());
    frontier
        .record_stop(ProductStop::FrontierExhausted)
        .unwrap();
    assert_eq!(
        frontier.state().unwrap().stopped.as_ref().unwrap().reason,
        StopReason::FrontierExhausted
    );
}

// r[verify chaoscontrol.campaign_policy.snapshot_eligibility.valid]
// r[verify chaoscontrol.campaign_policy.effects.valid]
#[test]
fn eligible_parents_restore_their_own_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let mut frontier = open_campaign(dir.path(), ScriptedIo::default());
    let mut rng = ChaCha8Rng::seed_from_u64(SEED);
    let plan = planned(&mut frontier, &mut rng, 0);
    let round = executable(frontier.authorize(plan).unwrap());
    assert_eq!(round.selected().unwrap().depth, 0);
    frontier
        .finish_round(round, &observation(1, &[Some(vec![50.0]), None]))
        .unwrap();
    // The child outranks both roots, so the exploit pick restores it at depth 1.
    let plan = planned(&mut frontier, &mut rng, 1);
    let round = executable(frontier.authorize(plan).unwrap());
    let selected = round.selected().unwrap();
    assert_eq!(selected.seq, 2);
    assert_eq!(selected.depth, 1);
}

// r[verify chaoscontrol.campaign_policy.stop.valid]
#[test]
fn product_stops_record_structural_classes_only() {
    let identity = AdapterIdentity::for_seed(SEED);
    let cases = [
        (
            ProductStop::CoveragePlateau,
            StopReason::ConsumerRequested {
                reason_id: identity.reason_id(AdapterReason::CoveragePlateau),
            },
        ),
        (
            ProductStop::ShortRunFinding,
            StopReason::ConsumerRequested {
                reason_id: identity.reason_id(AdapterReason::ShortRunFinding),
            },
        ),
        (
            ProductStop::Interrupted,
            StopReason::ConsumerRequested {
                reason_id: identity.reason_id(AdapterReason::OperatorSignal),
            },
        ),
    ];
    for (stop, expected) in cases {
        let dir = tempfile::tempdir().unwrap();
        let mut frontier = open_campaign(dir.path(), ScriptedIo::default());
        frontier.record_stop(stop).unwrap();
        let recorded = frontier.state().unwrap().stopped.clone().unwrap();
        assert_eq!(recorded.reason, expected);
        assert!(recorded.incomplete_search_acknowledged);
        // A second stop is idempotent for the product.
        frontier.record_stop(ProductStop::Interrupted).unwrap();
        assert_eq!(
            frontier.state().unwrap().stopped.clone().unwrap().reason,
            expected
        );
    }
    // Completion before the selection limit is not a structural completion.
    let dir = tempfile::tempdir().unwrap();
    let mut frontier = open_campaign(dir.path(), ScriptedIo::default());
    assert!(matches!(
        frontier.record_stop(ProductStop::Completed),
        Err(FrontierError::Adapter(_))
    ));
}

#[test]
fn zero_rounds_complete_without_selection() {
    let dir = tempfile::tempdir().unwrap();
    let config = ExplorerConfig {
        max_rounds: 0,
        ..config(FrontierPolicy::CampaignCandidate, dir.path())
    };
    let mut frontier =
        CampaignFrontier::open_with_io(&config, dir.path(), None, ScriptedIo::default()).unwrap();
    frontier.admit_root(root(Some(dummy_snapshot()))).unwrap();
    frontier
        .record_stop(product_stop(ExplorationStop::Completed))
        .unwrap();
    let identity = AdapterIdentity::for_seed(SEED);
    assert_eq!(
        frontier.state().unwrap().stopped.clone().unwrap().reason,
        StopReason::ConsumerRequested {
            reason_id: identity.reason_id(AdapterReason::CompletedWithoutSelection),
        }
    );
    assert_eq!(frontier.state().unwrap().selection_count, 0);
}

// ---------------------------------------------------------------------------
// Recovery.

// r[verify chaoscontrol.campaign_policy.history.valid]
#[test]
fn recovery_closes_the_outstanding_selection_and_starts_a_new_epoch() {
    let dir = tempfile::tempdir().unwrap();
    let binding;
    {
        let mut frontier = open_campaign(dir.path(), ScriptedIo::default());
        let mut rng = ChaCha8Rng::seed_from_u64(SEED);
        let plan = planned(&mut frontier, &mut rng, 0);
        let round = executable(frontier.authorize(plan).unwrap());
        frontier
            .finish_round(round, &observation(1, &[Some(vec![1.5]), None]))
            .unwrap();
        let settled = frontier.last_segment().unwrap();
        binding = CampaignJournalBinding::new(
            settled.epoch(),
            settled.seq(),
            *settled.digest().as_bytes(),
        );
        // Crash with the next selection published but never settled.
        let plan = planned(&mut frontier, &mut rng, 1);
        let _lost = executable(frontier.authorize(plan).unwrap());
    }
    let config = config(FrontierPolicy::CampaignCandidate, dir.path());
    let reopened =
        CampaignFrontier::open_with_io(&config, dir.path(), Some(&binding), ScriptedIo::default())
            .unwrap();
    let recovery = reopened.recovery();
    assert_eq!(recovery.recovered_epochs, 1);
    assert_eq!(recovery.closed_selections, 1);
    assert_eq!(recovery.closed_epoch, Some(0));
    assert_eq!(recovery.checkpoint_binding, CheckpointBinding::Matched);
    assert_eq!(reopened.epoch(), 1);
    drop(reopened);

    // The closed epoch replays as stopped; a third open closes nothing.
    let stale_binding = CampaignJournalBinding::new(0, binding.segment_seq, [9; 32]);
    let third = CampaignFrontier::open_with_io(
        &config,
        dir.path(),
        Some(&stale_binding),
        ScriptedIo::default(),
    )
    .unwrap();
    assert_eq!(third.recovery().closed_selections, 0);
    assert_eq!(third.recovery().closed_epoch, None);
    assert_eq!(
        third.recovery().checkpoint_binding,
        CheckpointBinding::Mismatched
    );
    assert_eq!(third.epoch(), 2);
}

#[test]
fn a_genesis_prune_replays_on_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let config = ExplorerConfig {
        max_frontier: 1,
        ..config(FrontierPolicy::CampaignCandidate, dir.path())
    };
    {
        let mut frontier =
            CampaignFrontier::open_with_io(&config, dir.path(), None, ScriptedIo::default())
                .unwrap();
        frontier.admit_root(root(Some(dummy_snapshot()))).unwrap();
        // Legacy keeps the first of the equal root scores; so does the genesis prune.
        let retained: Vec<u64> = frontier.retained().iter().map(|entry| entry.seq).collect();
        assert_eq!(retained, [0]);
    }
    let reopened =
        CampaignFrontier::open_with_io(&config, dir.path(), None, ScriptedIo::default()).unwrap();
    assert_eq!(reopened.recovery().recovered_epochs, 1);
    assert_eq!(reopened.epoch(), 1);
}

// r[verify chaoscontrol.campaign_policy.history.invalid]
#[test]
fn an_unknown_record_kind_fails_recovery_without_touching_the_journal() {
    let dir = tempfile::tempdir().unwrap();
    {
        let _frontier = open_campaign(dir.path(), ScriptedIo::default());
    }
    {
        let (mut journal, _) = CampaignJournal::open(dir.path()).unwrap();
        journal
            .append(&[JournalRecord::new(RecordKind::new(99), vec![0])])
            .unwrap();
    }
    let before = segment_count(dir.path(), 0);
    let config = config(FrontierPolicy::CampaignCandidate, dir.path());
    match CampaignFrontier::open_with_io(&config, dir.path(), None, ScriptedIo::default()) {
        Err(FrontierError::Replay(error)) => {
            assert_eq!(error.failure, ReplayFailure::UnknownKind(99));
        }
        Err(other) => panic!("unexpected {other}"),
        Ok(_) => panic!("unknown record replayed"),
    }
    assert_eq!(segment_count(dir.path(), 0), before);
    assert!(!dir
        .path()
        .join(JOURNAL_DIRECTORY_NAME)
        .join(crate::explorer::journal::epoch_directory_name(1))
        .exists());
}

// ---------------------------------------------------------------------------
// Trace schema. Legacy-versus-Campaign parity lives in `parity::tests`.

#[test]
fn a_campaign_trace_round_trips_through_its_schema() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(FrontierPolicy::CampaignCandidate, dir.path());
    let record = TraceRecord::header(&config, None);
    let line = serde_json::to_string(&record).unwrap();
    assert!(line.contains(trace::TRACE_SCHEMA));
    assert_eq!(serde_json::from_str::<TraceRecord>(&line).unwrap(), record);
    let unknown = line.replace("\"seed\"", "\"sneed\"");
    assert!(serde_json::from_str::<TraceRecord>(&unknown).is_err());
}
