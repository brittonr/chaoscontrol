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
        snapshot_tick: tick,
    })
}

fn proposal(score: f64) -> EntryProposal {
    EntryProposal {
        score,
        marker: None,
        assertion: false,
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
                retained: branch
                    .as_ref()
                    .map(|scores| RetainedBranch {
                        snapshot: dummy_snapshot(),
                        coverage: CoverageBitmap::new(),
                        schedule: FaultSchedule::new(),
                        depth,
                        proposals: scores.iter().copied().map(proposal).collect(),
                        digests: digests(BRANCH_TICKS),
                    })
                    .into_iter()
                    .collect(),
            })
            .collect(),
    }
}

#[test]
fn bootstrap_markers_and_each_round_have_separate_candidate_capacity() {
    use chaoscontrol_protocol::branch_marker::MAX_MARKERS_PER_RUN;

    const MARKER_ROUNDS: u64 = 2;
    for num_vms in [1, 3] {
        let dir = tempfile::tempdir().unwrap();
        let configuration = ExplorerConfig {
            num_vms,
            branch_factor: 1,
            max_rounds: MARKER_ROUNDS,
            max_frontier: 1,
            ..config(FrontierPolicy::Campaign, dir.path())
        };
        let mut frontier =
            CampaignFrontier::open_with_io(&configuration, dir.path(), None, ScriptedIo::default())
                .unwrap();
        let entries = num_vms * MAX_MARKERS_PER_RUN;
        let mut bootstrap = root(Some(dummy_snapshot()));
        bootstrap.proposals = (0..entries).map(|_| proposal(1.0)).collect();
        frontier.admit_root(bootstrap).unwrap();

        let mut rng = ChaCha8Rng::seed_from_u64(SEED);
        for round_index in 0..MARKER_ROUNDS {
            let plan = planned(&mut frontier, &mut rng, round_index);
            let authorized = executable(frontier.authorize(plan).unwrap());
            let depth = authorized.selected().unwrap().depth + 1;
            frontier
                .finish_round(authorized, &observation(depth, &[Some(vec![2.0; entries])]))
                .expect("every admitted VM marker fits beside bootstrap history");
            assert!(frontier.state().unwrap().outstanding().is_empty());
        }
        frontier.record_stop(ProductStop::Completed).unwrap();
        assert_eq!(
            frontier.state().unwrap().stopped().unwrap().reason,
            StopReason::SelectionLimitReached
        );
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
    let config = config(FrontierPolicy::Campaign, dir);
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
    assert_eq!(frontier.state().unwrap().outstanding().len(), 1);
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
    let base = config(FrontierPolicy::Campaign, dir.path());
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
    assert!(state.outstanding().is_empty());
    let retired = state
        .candidates()
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
        frontier.state().unwrap().stopped().unwrap().reason,
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
        let recorded = frontier.state().unwrap().stopped().cloned().unwrap();
        assert_eq!(recorded.reason, expected);
        assert!(recorded.incomplete_search_acknowledged);
        // A second stop is idempotent for the product.
        frontier.record_stop(ProductStop::Interrupted).unwrap();
        assert_eq!(
            frontier.state().unwrap().stopped().cloned().unwrap().reason,
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
        ..config(FrontierPolicy::Campaign, dir.path())
    };
    let mut frontier =
        CampaignFrontier::open_with_io(&config, dir.path(), None, ScriptedIo::default()).unwrap();
    frontier.admit_root(root(Some(dummy_snapshot()))).unwrap();
    frontier
        .record_stop(product_stop(ExplorationStop::Completed))
        .unwrap();
    let identity = AdapterIdentity::for_seed(SEED);
    assert_eq!(
        frontier.state().unwrap().stopped().cloned().unwrap().reason,
        StopReason::ConsumerRequested {
            reason_id: identity.reason_id(AdapterReason::CompletedWithoutSelection),
        }
    );
    assert_eq!(frontier.state().unwrap().selection_count(), 0);
}

/// Genesis control lineage of [`open_campaign`]: admission, the root candidate
/// batch, and one guidance record per root proposal (no prune below `MAX_FRONTIER`).
const GENESIS_LINEAGE: u64 = 2 + ROOT_SCORES.len() as u64;

/// Opens the Campaign backend with the Choregraph projection bound `bound`.
fn open_bounded(dir: &Path, bound: u64) -> CampaignFrontier<ScriptedIo> {
    let config = config(FrontierPolicy::Campaign, dir);
    let mut frontier =
        CampaignFrontier::open_with_io(&config, dir, None, ScriptedIo::default()).unwrap();
    frontier.tighten_projection_bound(bound);
    frontier.admit_root(root(Some(dummy_snapshot()))).unwrap();
    frontier
}

fn profile(dir: &Path) -> crate::campaign_adapter::CampaignProfile {
    crate::campaign_adapter::admit_config(
        &AdapterIdentity::for_seed(SEED),
        &config(FrontierPolicy::Campaign, dir),
        crate::campaign_adapter::BootstrapFacts {
            restorable_root: true,
        },
    )
    .unwrap()
}

/// Bound that holds the genesis, exactly one whole round, and the stop.
fn one_round_bound(dir: &Path) -> u64 {
    GENESIS_LINEAGE + profile(dir).round_control_events().unwrap() + 1
}

fn capacity_reason() -> StopReason {
    StopReason::ConsumerRequested {
        reason_id: campaign_choregraph::control_capacity_reason_id(),
    }
}

// r[verify chaoscontrol.campaign_policy.stop.valid]
#[test]
fn a_tight_projection_bound_ends_the_run_with_the_capacity_stop() {
    let dir = tempfile::tempdir().unwrap();
    let mut frontier = open_bounded(dir.path(), one_round_bound(dir.path()));
    let mut rng = ChaCha8Rng::seed_from_u64(SEED);
    // The first round fits whole and settles in full.
    let plan = planned(&mut frontier, &mut rng, 0);
    let round = executable(frontier.authorize(plan).unwrap());
    frontier
        .finish_round(round, &observation(1, &[Some(vec![1.5]), None]))
        .unwrap();
    let state = frontier.state().unwrap();
    assert!(state.outstanding().is_empty());
    assert_eq!(state.selection_count(), 1);
    // The second does not: the backend stops before drawing or publishing.
    let mut expected_rng = rng.clone();
    let stop = match frontier.begin_round(&mut rng, context(1)).unwrap() {
        RoundBegin::Stop(stop) => stop,
        RoundBegin::Planned(_) => panic!("a round was planned beyond the host capacity"),
    };
    assert_eq!(stop, ProductStop::ControlCapacityExhausted);
    assert_eq!(rng.next_u64(), expected_rng.next_u64());
    let class = exploration_stop(stop);
    assert_eq!(class, ExplorationStop::ControlCapacityExhausted);
    assert_eq!(
        serde_json::to_value(class).unwrap(),
        serde_json::json!("control_capacity_exhausted")
    );
    frontier.record_stop(product_stop(class)).unwrap();
    let recorded = frontier.state().unwrap().stopped().cloned().unwrap();
    assert_eq!(recorded.reason, capacity_reason());
    assert!(recorded.incomplete_search_acknowledged);
    assert_eq!(frontier.state().unwrap().selection_count(), 1);
    drop(frontier);

    // The journal replays the stopped epoch: nothing to close.
    let config = config(FrontierPolicy::Campaign, dir.path());
    let reopened =
        CampaignFrontier::open_with_io(&config, dir.path(), None, ScriptedIo::default()).unwrap();
    assert_eq!(reopened.recovery().recovered_epochs, 1);
    assert_eq!(reopened.recovery().closed_selections, 0);
    assert_eq!(reopened.recovery().closed_epoch, None);
    assert_eq!(reopened.epoch(), 1);
}

// r[verify chaoscontrol.campaign_policy.stop.invalid]
#[test]
fn one_event_short_of_a_whole_round_stops_before_the_first_selection() {
    let dir = tempfile::tempdir().unwrap();
    let mut frontier = open_bounded(dir.path(), one_round_bound(dir.path()) - 1);
    let mut rng = ChaCha8Rng::seed_from_u64(SEED);
    let mut expected_rng = rng.clone();
    assert!(matches!(
        frontier.begin_round(&mut rng, context(0)).unwrap(),
        RoundBegin::Stop(ProductStop::ControlCapacityExhausted)
    ));
    assert_eq!(rng.next_u64(), expected_rng.next_u64());
    frontier
        .record_stop(ProductStop::ControlCapacityExhausted)
        .unwrap();
    let state = frontier.state().unwrap();
    assert_eq!(state.selection_count(), 0);
    assert_eq!(state.stopped().unwrap().reason, capacity_reason());
}

// r[verify chaoscontrol.campaign_policy.stop.invalid]
#[test]
fn a_retirement_the_host_capacity_refuses_becomes_the_capacity_stop() {
    let dir = tempfile::tempdir().unwrap();
    // Room for the genesis and the stop only.
    let mut frontier = open_bounded(dir.path(), GENESIS_LINEAGE + 1);
    frontier.forget_snapshots();
    let mut rng = ChaCha8Rng::seed_from_u64(SEED);
    assert!(matches!(
        frontier.begin_round(&mut rng, context(0)).unwrap(),
        RoundBegin::Stop(ProductStop::ControlCapacityExhausted)
    ));
    // The refused retirement was never staged, and the backend stays live.
    let state = frontier.state().unwrap();
    assert!(state
        .candidates()
        .iter()
        .all(|candidate| candidate.status == CandidateStatus::Retained));
    frontier
        .record_stop(ProductStop::ControlCapacityExhausted)
        .unwrap();
    assert_eq!(
        frontier.state().unwrap().stopped().unwrap().reason,
        capacity_reason()
    );
}

/// Genesis graph of [`open_campaign`]: the root moment and the genesis lineage.
const GENESIS_GRAPH: u64 = 1 + GENESIS_LINEAGE;

/// Opens the Campaign backend with the Choregraph graph event limit `bound`;
/// the projection bound stays the derived one.
fn open_graph_bounded(
    dir: &Path,
    bound: u64,
) -> Result<CampaignFrontier<ScriptedIo>, FrontierError> {
    let config = config(FrontierPolicy::Campaign, dir);
    let mut frontier =
        CampaignFrontier::open_with_io(&config, dir, None, ScriptedIo::default()).unwrap();
    frontier.tighten_graph_bound(bound);
    frontier.admit_root(root(Some(dummy_snapshot())))?;
    Ok(frontier)
}

/// Graph event limit that holds the genesis, exactly one whole round with a
/// child moment per branch, and the stop.
fn one_round_graph_bound(dir: &Path) -> u64 {
    GENESIS_GRAPH + profile(dir).round_demand().unwrap().history_events + 1
}

/// Graph room Campaign exposes on the held projection (D75).
fn graph_room(frontier: &CampaignFrontier<ScriptedIo>) -> u64 {
    u64::from(frontier.projection().unwrap().graph_room())
}

/// Asserts the held projection's graph still holds Campaign's whole reserve:
/// no append of the Explorer consumed it.
fn assert_reserve_free(frontier: &CampaignFrontier<ScriptedIo>) {
    let held = frontier.projection().unwrap();
    assert!(
        u64::from(held.graph_events()) + u64::from(held.reserved_graph_events())
            <= u64::from(held.graph_max_events())
    );
}

// r[verify chaoscontrol.campaign_policy.stop.valid]
#[test]
fn a_tight_graph_bound_ends_the_run_with_the_capacity_stop() {
    let dir = tempfile::tempdir().unwrap();
    let mut frontier = open_graph_bounded(dir.path(), one_round_graph_bound(dir.path())).unwrap();
    let mut rng = ChaCha8Rng::seed_from_u64(SEED);
    // Campaign's exposed room holds exactly the round's appends: its control
    // events and one child moment per branch.
    let demand = profile(dir.path()).round_demand().unwrap();
    assert_eq!(graph_room(&frontier), demand.history_events);
    assert_eq!(frontier.projection().unwrap().reserved_graph_events(), 1);
    // The first round fits whole, child moments included, and settles in full.
    let plan = planned(&mut frontier, &mut rng, 0);
    assert_reserve_free(&frontier);
    let round = executable(frontier.authorize(plan).unwrap());
    assert_reserve_free(&frontier);
    frontier
        .finish_round(round, &observation(1, &[Some(vec![1.5]), Some(vec![0.5])]))
        .unwrap();
    assert_reserve_free(&frontier);
    let state = frontier.state().unwrap();
    assert!(state.outstanding().is_empty());
    assert_eq!(state.selection_count(), 1);
    assert!(graph_room(&frontier) < demand.history_events);
    // The second does not fit the graph: the backend stops before drawing.
    let mut expected_rng = rng.clone();
    assert!(matches!(
        frontier.begin_round(&mut rng, context(1)).unwrap(),
        RoundBegin::Stop(ProductStop::ControlCapacityExhausted)
    ));
    assert_eq!(rng.next_u64(), expected_rng.next_u64());
    frontier
        .record_stop(ProductStop::ControlCapacityExhausted)
        .unwrap();
    let recorded = frontier.state().unwrap().stopped().cloned().unwrap();
    assert_eq!(recorded.reason, capacity_reason());
    assert!(recorded.incomplete_search_acknowledged);
    let held = frontier.projection().unwrap();
    assert!(held.graph_events() <= held.graph_max_events());
    assert_eq!(held.reserved_graph_events(), 0);
}

// r[verify chaoscontrol.campaign_policy.stop.invalid]
#[test]
fn one_graph_event_short_of_a_whole_round_stops_before_the_first_selection() {
    let dir = tempfile::tempdir().unwrap();
    // Campaign's own reserve (a selection, its result, and the stop) still fits;
    // the round's child moments do not.
    let bound = one_round_graph_bound(dir.path()) - 1;
    assert!(GENESIS_GRAPH + 3 <= bound);
    let mut frontier = open_graph_bounded(dir.path(), bound).unwrap();
    // The exposed room is one event short of the round's appends.
    assert_eq!(
        graph_room(&frontier) + 1,
        profile(dir.path()).round_demand().unwrap().history_events
    );
    let mut rng = ChaCha8Rng::seed_from_u64(SEED);
    let mut expected_rng = rng.clone();
    assert!(matches!(
        frontier.begin_round(&mut rng, context(0)).unwrap(),
        RoundBegin::Stop(ProductStop::ControlCapacityExhausted)
    ));
    assert_eq!(rng.next_u64(), expected_rng.next_u64());
    frontier
        .record_stop(ProductStop::ControlCapacityExhausted)
        .unwrap();
    let state = frontier.state().unwrap();
    assert_eq!(state.selection_count(), 0);
    assert_eq!(state.stopped().unwrap().reason, capacity_reason());
}

// r[verify chaoscontrol.campaign_policy.stop.invalid]
#[test]
fn a_retirement_the_graph_bound_refuses_becomes_the_capacity_stop() {
    let dir = tempfile::tempdir().unwrap();
    // Room for the genesis and the stop only.
    let mut frontier = open_graph_bounded(dir.path(), GENESIS_GRAPH + 1).unwrap();
    frontier.forget_snapshots();
    let mut rng = ChaCha8Rng::seed_from_u64(SEED);
    assert!(matches!(
        frontier.begin_round(&mut rng, context(0)).unwrap(),
        RoundBegin::Stop(ProductStop::ControlCapacityExhausted)
    ));
    let state = frontier.state().unwrap();
    assert!(state
        .candidates()
        .iter()
        .all(|candidate| candidate.status == CandidateStatus::Retained));
    frontier
        .record_stop(ProductStop::ControlCapacityExhausted)
        .unwrap();
    assert_eq!(
        frontier.state().unwrap().stopped().unwrap().reason,
        capacity_reason()
    );
}

// r[verify chaoscontrol.campaign_policy.stop.invalid]
#[test]
fn a_genesis_without_room_for_its_stop_is_refused_before_the_journal() {
    let dir = tempfile::tempdir().unwrap();
    // The root moment and the admission leave no graph room for the stop.
    let Err(error) = open_graph_bounded(dir.path(), 2) else {
        panic!("a control root without room for its stop was admitted");
    };
    assert!(matches!(
        error,
        FrontierError::Adapter(AdapterError::Choregraph(
            campaign_choregraph::AdapterError::GraphCapacityExhausted
        ))
    ));
    // A projection bound of one holds the admission but not its stop.
    let config = config(FrontierPolicy::Campaign, dir.path());
    let mut frontier =
        CampaignFrontier::open_with_io(&config, dir.path(), None, ScriptedIo::default()).unwrap();
    frontier.tighten_projection_bound(1);
    assert!(matches!(
        frontier.admit_root(root(Some(dummy_snapshot()))),
        Err(FrontierError::Adapter(AdapterError::Choregraph(
            campaign_choregraph::AdapterError::ProjectionCapacityExhausted
        )))
    ));
    assert_eq!(frontier.last_segment(), None);
    assert!(frontier.state().is_none());
}

// ---------------------------------------------------------------------------
// Recovery.

// r[verify chaoscontrol.campaign_policy.history.valid]
// r[verify chaoscontrol.campaign_policy.journal_framing.valid]
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
    let config = config(FrontierPolicy::Campaign, dir.path());
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
        ..config(FrontierPolicy::Campaign, dir.path())
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
    let config = config(FrontierPolicy::Campaign, dir.path());
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

/// An epoch whose genesis names Campaign's earlier whole-state framing, or any other unknown
/// schema, fails recovery at its genesis record, before any moment or control record replays,
/// and recovery writes nothing.
// r[verify chaoscontrol.campaign_policy.journal_framing.invalid]
#[test]
fn an_epoch_recorded_before_the_chained_state_identity_fails_recovery_at_its_genesis() {
    // Recovery refuses the schema before it reads the epoch's history bound.
    const UNREAD_HISTORY_BOUND: u64 = 64;
    let cases = [
        (
            super::records::EARLIER_FRAMING_GENESIS_SCHEMA,
            ReplayFailure::EarlierStateFraming,
        ),
        (
            "chaoscontrol.campaign-genesis.v0",
            ReplayFailure::UnknownSchema,
        ),
    ];
    for (schema, failure) in cases {
        let dir = tempfile::tempdir().unwrap();
        {
            let (mut journal, _) = CampaignJournal::open(dir.path()).unwrap();
            assert_eq!(journal.begin_epoch().unwrap(), 0);
            let genesis = super::records::GenesisRecord {
                schema: schema.to_owned(),
                epoch: 0,
                seed: SEED,
                max_history_events: UNREAD_HISTORY_BOUND,
                previous_epoch_digest: None,
            };
            let record = super::records::encode(super::records::GENESIS, &genesis).unwrap();
            journal.append(&[record]).unwrap();
        }
        let before = segment_count(dir.path(), 0);
        let config = config(FrontierPolicy::Campaign, dir.path());
        match CampaignFrontier::open_with_io(&config, dir.path(), None, ScriptedIo::default()) {
            Err(FrontierError::Replay(error)) => {
                assert_eq!((error.epoch, error.record), (0, 0));
                assert_eq!(error.failure, failure);
            }
            Err(other) => panic!("unexpected {other}"),
            Ok(_) => panic!("{schema} replayed"),
        }
        assert_eq!(segment_count(dir.path(), 0), before);
        assert!(!dir
            .path()
            .join(JOURNAL_DIRECTORY_NAME)
            .join(crate::explorer::journal::epoch_directory_name(1))
            .exists());
    }
}

/// Campaign state identities of one fixed journal: after the genesis segment and after the
/// first published selection. Every recorded control event names such identities, so a
/// Campaign or adapter change that moves either one leaves earlier journals unable to replay:
/// bump `GENESIS_SCHEMA`, refuse the previous schema, and re-pin both values.
// r[verify chaoscontrol.campaign_policy.journal_framing]
#[test]
fn the_genesis_schema_names_the_pinned_state_identities() {
    const GENESIS_STATE: &str =
        "State:9631f13464bf8bf2ed4d1262c9977f06f8893370b4f1a3d29578506abe167714";
    const SELECTED_STATE: &str =
        "State:f8eaf159e821e6e0b41b347e1b97ea21d72b2ae10cbddaca14c705621d3683fd";
    let dir = tempfile::tempdir().unwrap();
    let mut frontier = open_campaign(dir.path(), ScriptedIo::default());
    let genesis = frontier.state().unwrap().state_id().to_string();
    let mut rng = ChaCha8Rng::seed_from_u64(SEED);
    let plan = planned(&mut frontier, &mut rng, 0);
    let _round = executable(frontier.authorize(plan).unwrap());
    let selected = frontier.state().unwrap().state_id().to_string();
    assert_ne!(genesis, selected);
    assert_eq!(
        (genesis.as_str(), selected.as_str()),
        (GENESIS_STATE, SELECTED_STATE),
        "journal state identities moved: bump GENESIS_SCHEMA before re-pinning"
    );
}

// ---------------------------------------------------------------------------
// One campaign per graph; the current graph; no control drafts (Campaign D82-D84).

/// Asserts the committed session presents its current graph and holds no
/// Campaign control draft: the held projection covers exactly the session's
/// graph (so `validate_control_advance` never sees an older graph), every
/// control event of the graph is on the lineage, and Campaign's exposed
/// reserve is only the settlements of outstanding selections and the stop.
fn assert_current_graph_without_drafts(frontier: &CampaignFrontier<ScriptedIo>) {
    let session = frontier.control_session().expect("genesis committed");
    let projection = session.projection();
    assert_eq!(session.graph().len(), projection.graph_events());
    assert_eq!(projection.state(), session.state());
    assert_eq!(projection.control_events().len(), session.payloads().len());
    let state = projection.state();
    let owed =
        u32::try_from(state.outstanding().len()).unwrap() + u32::from(state.stopped().is_none());
    assert_eq!(projection.reserved_control_events(), owed);
    assert_eq!(projection.reserved_graph_events(), owed);
}

/// Admits `admission` as a control root on `frontier`'s committed graph.
fn admit_second_root(
    frontier: &CampaignFrontier<ScriptedIo>,
    admission: campaign_core::CampaignControlEvent,
    seed: u64,
) -> Option<campaign_choregraph::AdapterError> {
    let identity = AdapterIdentity::for_seed(seed);
    campaign_choregraph::admit_control_root(
        frontier.control_session().unwrap().graph(),
        admission,
        identity.projection_schema_id,
        identity.adapter_id,
        identity.control_domain_id,
    )
    .err()
}

// r[verify chaoscontrol.campaign_policy.history.valid]
// r[verify chaoscontrol.campaign_policy.history.invalid]
#[test]
fn every_staged_publication_matches_cold_replay_and_rejects_stale_controls() {
    use crate::campaign_adapter::{decay_revision, load_projection, GuidanceFacts};
    use campaign_choregraph::{ControlProjectionRequest, ParentChoices};

    let dir = tempfile::tempdir().unwrap();
    let frontier = open_campaign(dir.path(), ScriptedIo::default());
    let committed = frontier.control_session().unwrap();
    let committed_projection = committed.projection().clone();
    let mut staged = committed.clone();
    let identity = AdapterIdentity::for_seed(SEED);
    for increment in [1_u32, 2, 3] {
        let request = decay_revision(
            &identity,
            staged.state().candidates()[0]
                .latest_guidance
                .as_ref()
                .unwrap(),
            GuidanceFacts {
                score: ROOT_SCORES[0],
                times_selected: increment,
                entry_seq: 0,
            },
        )
        .unwrap();
        let event = campaign_core::revise_guidance(staged.state(), request).unwrap();
        let stale = event.clone();
        let publication = staged.publish(event).unwrap();
        let choices = ParentChoices::new();
        let replayed = load_projection(&ControlProjectionRequest {
            graph: staged.graph(),
            branch: &publication.branch_plan().after,
            parent_choices: &choices,
            maximum_events: staged.projection().maximum_events(),
            expected_binding: staged.binding().unwrap(),
            payloads: staged.payloads(),
        })
        .unwrap();
        assert_eq!(staged.projection(), &replayed);
        assert_eq!(staged.state(), replayed.state());

        assert!(staged.publish(stale).is_err());
        assert_eq!(staged.projection(), &replayed);
        assert_eq!(staged.state(), replayed.state());
        assert_eq!(staged.graph().len(), replayed.graph_events());
        assert_eq!(staged.binding().unwrap(), *replayed.binding());
        assert_eq!(staged.payloads().len(), replayed.control_events().len());
        assert_current_graph_without_drafts(&frontier);
    }
    let before_seal = staged.projection().clone();
    staged.seal().unwrap();
    assert_eq!(staged.projection(), &before_seal);
    assert_eq!(committed.projection(), &committed_projection);
    assert_eq!(committed.state(), committed_projection.state());
}

// r[verify chaoscontrol.campaign_policy.history.valid]
#[test]
fn every_commit_presents_the_current_graph_and_leaves_no_control_draft() {
    let dir = tempfile::tempdir().unwrap();
    let mut frontier = open_campaign(dir.path(), ScriptedIo::default());
    let mut rng = ChaCha8Rng::seed_from_u64(SEED);
    assert_current_graph_without_drafts(&frontier);
    for (round, branches) in [(0, [Some(vec![1.5]), None]), (1, [None, None])] {
        let plan = planned(&mut frontier, &mut rng, round);
        assert_current_graph_without_drafts(&frontier);
        let authorized = executable(frontier.authorize(plan).unwrap());
        assert_current_graph_without_drafts(&frontier);
        assert_eq!(frontier.state().unwrap().outstanding().len(), 1);
        frontier
            .finish_round(authorized, &observation(1, &branches))
            .unwrap();
        assert_current_graph_without_drafts(&frontier);
        assert!(frontier.state().unwrap().outstanding().is_empty());
    }
    let session = frontier.control_session().unwrap();
    // Child moments are graph events outside the control lineage.
    assert!(usize::try_from(session.graph().len()).unwrap() > session.payloads().len());
}

// r[verify chaoscontrol.campaign_policy.history.invalid]
#[test]
fn the_explorers_graph_admits_no_second_campaign_root() {
    let dir = tempfile::tempdir().unwrap();
    let mut frontier = open_campaign(dir.path(), ScriptedIo::default());
    let mut rng = ChaCha8Rng::seed_from_u64(SEED);
    let plan = planned(&mut frontier, &mut rng, 0);
    let round = executable(frontier.authorize(plan).unwrap());
    frontier
        .finish_round(round, &observation(1, &[Some(vec![1.5]), None]))
        .unwrap();
    // The shell itself never drafts a second root: genesis refuses a second
    // admission before Campaign sees it.
    assert!(matches!(
        frontier.admit_root(root(Some(dummy_snapshot()))),
        Err(FrontierError::Sequence("campaign root admitted twice"))
    ));
    let lineage: Vec<_> = frontier
        .control_session()
        .unwrap()
        .payloads()
        .iter()
        .map(|payload| payload.history_event_id)
        .collect();
    let own = frontier.control_session().unwrap().payloads()[0]
        .control_event
        .clone();
    // Another campaign under the same envelope names the first event of this
    // campaign's lineage in identity order.
    let other_dir = tempfile::tempdir().unwrap();
    let other_seed = SEED + 1;
    let other_config = ExplorerConfig {
        seed: other_seed,
        ..config(FrontierPolicy::Campaign, other_dir.path())
    };
    let mut other = CampaignFrontier::open_with_io(
        &other_config,
        other_dir.path(),
        None,
        ScriptedIo::default(),
    )
    .unwrap();
    other.admit_root(root(Some(dummy_snapshot()))).unwrap();
    let foreign = other.control_session().unwrap().payloads()[0]
        .control_event
        .clone();
    assert_ne!(foreign, own);
    assert_eq!(
        admit_second_root(&frontier, foreign, other_seed),
        Some(campaign_choregraph::AdapterError::SharedControlGraph {
            event_id: *lineage.iter().min().unwrap()
        })
    );
    // Retrying this campaign's own root after later publications names the
    // first later event.
    assert_eq!(
        admit_second_root(&frontier, own, SEED),
        Some(campaign_choregraph::AdapterError::SharedControlGraph {
            event_id: *lineage[1..].iter().min().unwrap()
        })
    );
    assert_current_graph_without_drafts(&frontier);
}

// ---------------------------------------------------------------------------
// Publication only on durably committed branch values (Campaign D91).

// r[verify chaoscontrol.campaign_policy.publication_fence.invalid]
#[test]
fn an_uncommitted_segment_leaves_no_draft_and_nothing_publishes_over_it() {
    let dir = tempfile::tempdir().unwrap();
    let io = ScriptedIo::default();
    let mut frontier = open_campaign(dir.path(), io.clone());
    let mut rng = ChaCha8Rng::seed_from_u64(SEED);
    let plan = planned(&mut frontier, &mut rng, 0);
    let round = executable(frontier.authorize(plan).unwrap());
    let session = frontier.control_session().unwrap();
    let graph_events = session.graph().len();
    let lineage = session.payloads().len();
    let state = session.state().clone();
    io.arm(PublicationPhase::WritePayload, io::ErrorKind::Other, 1);
    // The settlement segment stages several publications (result, candidate,
    // guidance), each on the branch value the one before it planned; none of
    // them is durable until the segment commits.
    let error = frontier
        .finish_round(round, &observation(1, &[Some(vec![1.5]), None]))
        .expect_err("uncommitted settlement segment");
    assert!(matches!(error, FrontierError::Journal(_)), "{error}");
    assert!(!frontier.journal().is_poisoned());
    // Their graph events and branch moves are dropped together: the committed
    // session holds no draft whose branch update did not become durable.
    let session = frontier.control_session().unwrap();
    assert_eq!(session.graph().len(), graph_events);
    assert_eq!(session.payloads().len(), lineage);
    assert_eq!(session.state(), &state);
    assert_current_graph_without_drafts(&frontier);
    // Nothing is planned or published over the lost branch values.
    assert!(matches!(
        frontier.begin_round(&mut rng, context(1)),
        Err(FrontierError::Halted)
    ));
    assert!(matches!(
        frontier.record_stop(ProductStop::Interrupted),
        Err(FrontierError::Halted)
    ));
}

// ---------------------------------------------------------------------------
// Trace schema. Legacy-versus-Campaign parity lives in `parity::tests`.

#[test]
fn a_campaign_trace_round_trips_through_its_schema() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(FrontierPolicy::Campaign, dir.path());
    let record = TraceRecord::header(&config, None);
    let line = serde_json::to_string(&record).unwrap();
    assert!(line.contains(trace::TRACE_SCHEMA));
    assert_eq!(serde_json::from_str::<TraceRecord>(&line).unwrap(), record);
    let unknown = line.replace("\"seed\"", "\"sneed\"");
    assert!(serde_json::from_str::<TraceRecord>(&unknown).is_err());
}
