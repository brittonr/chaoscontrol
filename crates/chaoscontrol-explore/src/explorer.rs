//! The main exploration loop — coverage-guided fault schedule search.

use crate::branch_recovery::{
    decide_after_watchdog, explore_error_watchdog_vcpu, BranchOutcome as RecoveredBranchOutcome,
    RecoverySnapshot, RoundWatchdogLedger, WatchdogDecision, MAX_BRANCH_WATCHDOG_ATTEMPTS,
};
use crate::campaign_adapter::BranchSlot;
use crate::checkpoint::CampaignJournalBinding;
use crate::checkpoint::{
    save_checkpoint, CheckpointConfig, CheckpointError, ExplorationCheckpoint, SerializableBug,
};
use crate::corpus::{BugReport, Corpus, CorpusEntry};
use crate::coverage::{CoverageBitmap, CoverageCollector, CoverageStats};
use crate::frontier::FrontierEntry;
use crate::input_tree;
use crate::marker_branching::{
    frontier_metadata, frontier_observations as marker_observations, marker_score,
    update_hit_counts,
};
use crate::mutator::{MutationConfig, ScheduleMutator};
use crate::worker::{BranchWork, WorkerPool};
use chaoscontrol_fault::oracle::OracleReport;
use chaoscontrol_fault::schedule::FaultSchedule;
use chaoscontrol_protocol::COVERAGE_BITMAP_ADDR;
use chaoscontrol_vmm::controller::{
    SimulationConfig, SimulationController, SimulationSnapshot, DEFAULT_RANDOM_FAULTS,
};
use chaoscontrol_vmm::scheduler::{ScheduleVariant, SchedulingStrategy};
use chaoscontrol_vmm::vm::VmConfig;
use log::{debug, info, warn};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use snafu::Snafu;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::SyncSender;
use std::sync::Arc;
use std::time::Instant;

mod frontier_backend;
mod frontier_policy;
pub mod journal;
mod journal_authority;
mod round_stops;

use frontier_backend::digests::{
    base_set_id, coverage_digest, oracle_digest, schedule_digest, variant_digest,
};
pub use frontier_backend::parity;
use frontier_backend::trace::{FrontierTrace, TraceJournal, TraceRecord};
use frontier_backend::{
    exploration_stop, product_stop, Authorization, AuthorizedRound, BranchDigests, BranchOutcome,
    CampaignFrontier, CheckpointBinding, EntryProposal, FindingFacts, FrontierBackend,
    LegacyFrontier, RetainedBranch, RootAdmission, RoundBegin, RoundContext, RoundMode,
    RoundObservation,
};
pub use frontier_backend::{FrontierError, ReplayError, ReplayFailure};

pub use frontier_policy::{
    ExplorationStop, FrontierPolicy, FrontierPolicyParseError, FrontierPolicyReceipt,
    CAMPAIGN_POLICY_NAME, EXHAUSTIVE_SEARCH_NON_CLAIM, LEGACY_DIAGNOSTIC_BANNER,
    LEGACY_DIAGNOSTIC_POLICY_NAME, RETIRED_CAMPAIGN_CANDIDATE_POLICY_NAME,
    RETIRED_LEGACY_POLICY_NAME,
};

/// Errors from the exploration engine.
#[derive(Debug, Snafu)]
pub enum ExploreError {
    #[snafu(display("VM error"), context(false))]
    Vm {
        source: chaoscontrol_vmm::vm::VmError,
    },

    #[snafu(display("Configuration error: {message}"))]
    Config { message: String },

    #[snafu(display("checkpoint persistence failed"), context(false))]
    Checkpoint { source: CheckpointError },

    #[snafu(display("frontier policy failed: {source}"), context(false))]
    Frontier { source: FrontierError },

    #[snafu(display("frontier trace write failed"))]
    Trace { source: std::io::Error },

    #[snafu(display(
        "the legacy-diagnostic frontier policy requires output_dir so its frontier-policy.json receipt, checkpoint, and report are marked release-ineligible"
    ))]
    LegacyDiagnosticWithoutOutput,

    #[snafu(display("frontier-policy.json receipt write failed"))]
    FrontierPolicyReceipt { source: std::io::Error },
}

/// How the explorer branches the execution tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExplorationMode {
    /// Mutate fault schedules — the original mode.
    /// Branches by varying which faults (partitions, crashes, etc.)
    /// are injected at what times.
    #[default]
    FaultSchedule,

    /// Branch at random choice points — the Antithesis input tree model.
    /// Records what `random_choice()` / `get_random()` calls the guest
    /// makes, then re-runs from the same snapshot with different values
    /// at selected decision points.
    InputTree,

    /// Alternate between fault schedule mutation (even rounds) and
    /// input tree exploration (odd rounds).  Both modes share the same
    /// frontier — an input tree branch can be further explored via
    /// fault mutation and vice versa.
    Hybrid,
}

/// Configuration for an exploration session.
#[derive(Clone)]
pub struct ExplorerConfig {
    /// Number of VMs per simulation.
    pub num_vms: usize,
    /// Per-VM config.
    pub vm_config: VmConfig,
    /// Kernel path.
    pub kernel_path: String,
    /// Optional initrd.
    pub initrd_path: Option<String>,
    /// Master seed.
    pub seed: u64,
    /// How many branches to explore from each snapshot.
    pub branch_factor: usize,
    /// How many ticks to run each branch.
    pub ticks_per_branch: u64,
    /// Max total exploration rounds.
    pub max_rounds: u64,
    /// Max frontier size.
    pub max_frontier: usize,
    /// Exits per VM per scheduling round (passed to SimulationController).
    pub quantum: u64,
    /// Scheduling strategy for multi-vCPU VMs.
    pub scheduling_strategy: SchedulingStrategy,
    /// Mutation config.
    pub mutation: MutationConfig,
    /// Exploration mode — how to branch the execution tree.
    pub exploration_mode: ExplorationMode,
    /// Guest physical address of coverage bitmap (0 = blind mode).
    pub coverage_gpa: u64,
    /// Output directory for checkpoints, reports, and the Campaign journal.
    /// `run` rejects absent or malformed root names before KVM work.
    pub output_dir: Option<String>,
    /// Optional disk image path for virtio-blk devices.
    ///
    /// When set, each VM's block device is loaded from this file.
    pub disk_image_path: Option<String>,
    /// Maximum ticks for bootstrap (kernel boot + guest init).
    /// Bootstrap runs until `setup_complete` or this limit.
    /// Default: 10_000 (enough for kernel boot + guest init).
    pub bootstrap_budget: u64,

    /// Directory for determinism log files.
    ///
    /// When set, per-VM `.dlog` files are written during exploration.
    /// Primarily useful for debugging determinism issues on a specific
    /// seed — produces ~6 MB per 100K exits per VM.
    pub dlog_dir: Option<std::path::PathBuf>,
    /// Emit a full RegisterDump dlog record every N VM exits.
    pub dlog_register_interval: u64,
    /// Hash guest memory pages at snapshot boundaries.
    pub dlog_memory_hash: bool,
    /// Number of parallel worker threads for branch execution.
    ///
    /// - `1` (default): sequential execution, identical to previous behavior.
    /// - `N > 1`: N worker controllers run branches in parallel.
    /// - `0`: auto-detect based on available cores.
    pub num_workers: usize,
    /// Stop after this many consecutive rounds with 0 new edges and 0 new bugs.
    /// 0 = never stop early due to stale rounds (run all max_rounds).
    /// Default: 10.
    pub stale_round_limit: u64,
    /// Enable per-branch vCPU schedule diversity.
    ///
    /// When `true`, each branch gets a different scheduler seed so
    /// vCPUs interleave differently across branches. The schedule
    /// fingerprint is injected into the coverage bitmap so different
    /// interleavings are treated as distinct.
    ///
    /// Default: `true` when `vm_config.num_vcpus > 1`, `false` otherwise.
    /// No-op when `num_vcpus == 1`.
    pub schedule_diversity: bool,
    /// Rare-edge threshold: edges with global hit count ≤ this value
    /// are considered "rare" and get boosted frontier scores.
    /// Default: 3.
    pub rare_edge_threshold: u8,
    /// Score multiplier per rare edge in frontier scoring.
    /// Default: 5.0.
    pub rare_edge_weight: f64,
    /// Consecutive stale rounds before havoc mutations activate.
    /// Set to 0 to disable havoc. Default: `stale_round_limit / 2`
    /// (computed at runtime if left at 0).
    pub havoc_after_stale: u64,
    /// Range of mutations per variant in havoc mode: `[min, max]`.
    /// Default: `[4, 16]`.
    pub havoc_mutations: [u32; 2],
    /// Optional helical scenario config. When set, the initial fault
    /// schedule for each run is materialized from this scenario instead
    /// of being generated randomly.
    pub scenario: Option<chaoscontrol_fault::scenario::ScenarioConfig>,
    /// Emit one JSONL metrics record per exploration round.
    pub emit_metrics: bool,
    /// Optional JSONL metrics output path. Stdout is used when omitted.
    pub metrics_file: Option<std::path::PathBuf>,
    /// Frontier policy that selects each round's expansion.
    pub frontier_policy: FrontierPolicy,
    /// Whether every system run of this exploration injects seed-derived
    /// random faults in addition to its schedule.
    ///
    /// Defaults to the system-run default, which is on. The run records the
    /// value in its checkpoint and in each bug it finds, so resumption,
    /// minimization, and reproduction use the setting the run used.
    pub random_faults: bool,
}

impl Default for ExplorerConfig {
    fn default() -> Self {
        Self {
            num_vms: 2,
            vm_config: VmConfig::default(),
            kernel_path: String::new(),
            initrd_path: None,
            seed: 42,
            branch_factor: 8,
            ticks_per_branch: 1000,
            max_rounds: 100,
            max_frontier: 50,
            quantum: 100,
            scheduling_strategy: SchedulingStrategy::RoundRobin,
            mutation: MutationConfig::default(),
            exploration_mode: ExplorationMode::default(),
            coverage_gpa: COVERAGE_BITMAP_ADDR,
            output_dir: None,
            disk_image_path: None,
            bootstrap_budget: 10_000,
            dlog_dir: None,
            dlog_register_interval: 0,
            dlog_memory_hash: false,
            num_workers: 1,
            stale_round_limit: 10,
            schedule_diversity: false,
            rare_edge_threshold: 3,
            rare_edge_weight: 5.0,
            havoc_after_stale: 0,
            havoc_mutations: [4, 16],
            scenario: None,
            emit_metrics: false,
            metrics_file: None,
            frontier_policy: FrontierPolicy::default(),
            random_faults: DEFAULT_RANDOM_FAULTS,
        }
    }
}

const MINIMUM_SMP_VCPUS: usize = 2;

/// Branch slot index of the `index`-th branch of a round.
fn branch_slot_index(index: usize) -> Result<u32, ExploreError> {
    u32::try_from(index).map_err(|_| ExploreError::Config {
        message: "branch index exceeds the branch-slot encoding".to_string(),
    })
}

/// Banner and release-ineligible `frontier-policy.json` receipt of an
/// explicit legacy-diagnostic run, emitted before bootstrap so that even an
/// aborted diagnostic run leaves its outputs marked.
// r[impl chaoscontrol.campaign_policy.rollback]
fn announce_legacy_diagnostic(
    config: &ExplorerConfig,
    output_dir: &std::path::Path,
) -> Result<(), ExploreError> {
    eprintln!("{LEGACY_DIAGNOSTIC_BANNER}");
    warn!(
        "frontier_policy={}: diagnostic rollback run; outputs are not release eligible",
        config.frontier_policy
    );
    FrontierPolicyReceipt::for_config(config)
        .write(output_dir)
        .map_err(|source| ExploreError::FrontierPolicyReceipt { source })
}

/// Outcome of one round of the exploration loop.
enum RoundRun {
    /// The round ran (or was closed without execution).
    Completed(RoundReport),
    /// The frontier policy stopped before selecting.
    Stopped(ExplorationStop),
}

/// The frontier backend of one explorer.
enum ExplorerFrontier {
    /// Legacy in-process frontier.
    Legacy(LegacyFrontier),
    /// Campaign policy before `run` opened the durable journal.
    CampaignUnopened,
    /// Campaign-backed frontier.
    Campaign(Box<CampaignFrontier>),
}

impl ExplorerFrontier {
    fn for_config(config: &ExplorerConfig) -> Self {
        match config.frontier_policy {
            FrontierPolicy::LegacyDiagnostic => {
                Self::Legacy(LegacyFrontier::new(config.max_frontier))
            }
            FrontierPolicy::Campaign => Self::CampaignUnopened,
        }
    }

    fn backend(&mut self) -> Result<&mut dyn FrontierBackend, FrontierError> {
        match self {
            Self::Legacy(frontier) => Ok(frontier),
            Self::Campaign(frontier) => Ok(frontier.as_mut()),
            Self::CampaignUnopened => Err(FrontierError::Sequence(
                "campaign frontier used before the journal was opened",
            )),
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::Legacy(frontier) => frontier.len(),
            Self::Campaign(frontier) => frontier.len(),
            Self::CampaignUnopened => 0,
        }
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Journal epoch this run writes (0 for the legacy policy).
    fn epoch(&self) -> u32 {
        match self {
            Self::Campaign(frontier) => frontier.epoch(),
            Self::Legacy(_) | Self::CampaignUnopened => 0,
        }
    }

    // r[impl chaoscontrol.campaign_policy.progress]
    /// Last committed journal segment, bound into `checkpoint.json`.
    fn journal_binding(&self) -> Option<CampaignJournalBinding> {
        match self {
            Self::Campaign(frontier) => frontier.last_segment().map(|segment| {
                CampaignJournalBinding::new(
                    segment.epoch(),
                    segment.seq(),
                    *segment.digest().as_bytes(),
                )
            }),
            Self::Legacy(_) | Self::CampaignUnopened => None,
        }
    }
}

const fn schedule_diversity_enabled(enabled: bool, num_vcpus: usize) -> bool {
    enabled && num_vcpus >= MINIMUM_SMP_VCPUS
}

fn branch_work_from_variants(
    variants: &[(FaultSchedule, Option<ScheduleVariant>)],
) -> Vec<BranchWork> {
    variants
        .iter()
        .enumerate()
        .map(|(branch_index, (schedule, schedule_variant))| BranchWork {
            schedule: schedule.clone(),
            branch_index,
            schedule_variant: schedule_variant.clone(),
        })
        .collect()
}

/// The exploration engine.
pub struct Explorer {
    config: ExplorerConfig,
    frontier: ExplorerFrontier,
    corpus: Corpus,
    mutator: ScheduleMutator,
    coverage: CoverageCollector,
    rng: ChaCha8Rng,
    /// Reusable controller — avoids 5s kernel boot per branch.
    /// Created once during bootstrap, then restored from snapshots.
    controller: Option<SimulationController>,
    /// Stats tracking.
    rounds_completed: u64,
    total_branches_run: u64,
    /// Per-round history.
    round_history: Vec<RoundHistory>,
    /// Per-VM base memory images for incremental snapshots.
    ///
    /// Set after the bootstrap snapshot. Each `Arc<Vec<u8>>` is shared
    /// by all overlay snapshots in the frontier, so cloning a snapshot
    /// copies only the dirty-page overlay.
    memory_bases: Vec<Arc<Vec<u8>>>,
    /// Worker pool for parallel branch execution (None = sequential).
    worker_pool: Option<WorkerPool>,
    /// Optional event sink for live dashboard updates.
    event_sink: Option<SyncSender<crate::dashboard_types::DashboardEvent>>,
    /// Dedup keys for bugs already in the corpus.
    seen_dedup_keys: BTreeSet<u64>,
    /// Bugs found in branches with no new coverage (not stored in corpus).
    standalone_bugs: Vec<BugReport>,
    /// Consecutive rounds with 0 new edges and 0 new bugs.
    consecutive_stale_rounds: u64,
    /// Prior observations per stable branch-marker identity.
    marker_hits: BTreeMap<String, u32>,
    /// Materialized phase summary (stored at bootstrap time).
    scenario_summary: Option<chaoscontrol_fault::scenario::PhaseSummary>,
    metrics_sink: Option<std::io::BufWriter<std::fs::File>>,
    /// First stop of the run, once one was reached.
    stop: Option<ExplorationStop>,
    /// Product frontier decision trace (`output_dir/frontier-trace.jsonl`).
    trace: Option<FrontierTrace>,
    /// Journal position the resumed checkpoint was bound to.
    resumed_journal: Option<CampaignJournalBinding>,
    /// The resumed checkpoint carried rounds from a legacy-diagnostic run, so
    /// no later output of this run is release eligible whatever its policy.
    inherits_diagnostic_rounds: bool,
    /// Bootstrap state used to replace a controller poisoned by a branch watchdog.
    recovery: Option<RecoverySnapshot>,
}

impl Explorer {
    /// Create a new explorer with the given configuration.
    pub fn new(config: ExplorerConfig) -> Self {
        if config.kernel_path.is_empty() {
            warn!("ExplorerConfig has empty kernel_path — exploration will fail");
        }

        let frontier = ExplorerFrontier::for_config(&config);
        let corpus = Corpus::new();
        let mutator = ScheduleMutator::new(config.seed);
        let coverage = CoverageCollector::new(config.coverage_gpa);
        let rng = ChaCha8Rng::seed_from_u64(config.seed);

        Self {
            config,
            frontier,
            corpus,
            mutator,
            coverage,
            rng,
            controller: None,
            rounds_completed: 0,
            total_branches_run: 0,
            round_history: Vec::new(),
            memory_bases: Vec::new(),
            worker_pool: None,
            event_sink: None,
            seen_dedup_keys: BTreeSet::new(),
            standalone_bugs: Vec::new(),
            consecutive_stale_rounds: 0,
            marker_hits: BTreeMap::new(),
            scenario_summary: None,
            metrics_sink: None,
            stop: None,
            trace: None,
            resumed_journal: None,
            inherits_diagnostic_rounds: false,
            recovery: None,
        }
    }

    /// Whether this run's outputs may satisfy release, dogfood, and
    /// KVM-matrix gates: only Campaign runs that never carried
    /// legacy-diagnostic rounds.
    // r[impl chaoscontrol.campaign_policy.rollback]
    #[must_use]
    pub fn release_eligible(&self) -> bool {
        self.config.frontier_policy.release_eligible() && !self.inherits_diagnostic_rounds
    }

    /// All bugs found: corpus bugs + standalone bugs from zero-coverage branches.
    fn all_bugs(&self) -> impl Iterator<Item = &BugReport> {
        self.corpus
            .entries()
            .iter()
            .flat_map(|entry| entry.bugs_found.iter())
            .chain(self.standalone_bugs.iter())
    }

    /// Set an event sink for dashboard updates.
    ///
    /// When set, the explorer emits `DashboardEvent`s at round boundaries.
    /// Uses `try_send` — events are silently dropped if the channel is full.
    pub fn set_event_sink(&mut self, sink: SyncSender<crate::dashboard_types::DashboardEvent>) {
        self.event_sink = Some(sink);
    }

    /// Emit an event to the dashboard (if a sink is configured).
    fn emit_event(&self, event: crate::dashboard_types::DashboardEvent) {
        if let Some(ref sink) = self.event_sink {
            let _ = sink.try_send(event);
        }
    }

    /// Record the run's first stop and emit a Finished event.
    ///
    /// The legacy loop can emit `completed` after an in-loop stop on the last
    /// round; the report keeps the first stop.
    fn finish(&mut self, stop: ExplorationStop) {
        self.stop.get_or_insert(stop);
        self.emit_finished(stop.as_str());
    }

    /// Emit a Finished event.
    fn emit_finished(&self, reason: &str) {
        self.emit_event(crate::dashboard_types::DashboardEvent::Finished {
            total_rounds: self.rounds_completed,
            total_branches: self.total_branches_run,
            total_edges: self.coverage.stats().total_edges,
            total_bugs: self.all_bugs().count(),
            reason: reason.to_string(),
            from_seed: None,
        });
    }

    fn phase_totals_for(results: &[(usize, BranchResult, FaultSchedule)]) -> BranchTimings {
        results
            .iter()
            .fold(BranchTimings::default(), |mut total, (_, result, _)| {
                total.restore_ms += result.timings.restore_ms;
                total.run_ms += result.timings.run_ms;
                total.snapshot_ms += result.timings.snapshot_ms;
                total.coverage_ms += result.timings.coverage_ms;
                total
            })
    }

    fn emit_metrics_line(&mut self, line: &MetricsLine) {
        if !self.config.emit_metrics {
            return;
        }

        let Ok(json) = serde_json::to_string(line) else {
            warn!("failed to serialize metrics line");
            return;
        };

        if self.metrics_sink.is_none() {
            if let Some(path) = &self.config.metrics_file {
                match std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                {
                    Ok(file) => self.metrics_sink = Some(std::io::BufWriter::new(file)),
                    Err(e) => {
                        warn!("failed to open metrics file {}: {}", path.display(), e);
                        return;
                    }
                }
            }
        }

        if let Some(sink) = self.metrics_sink.as_mut() {
            use std::io::Write;
            if let Err(e) = writeln!(sink, "{}", json).and_then(|_| sink.flush()) {
                warn!("failed to write metrics line: {}", e);
            }
        } else {
            eprintln!("{}", json);
        }
    }

    /// Run the full exploration loop.
    ///
    /// Returns the final report with all bugs found, coverage stats, etc.
    pub fn run(&mut self) -> Result<ExplorationReport, ExploreError> {
        // r[impl chaoscontrol.campaign_policy.rollback]
        if self.config.frontier_policy.is_legacy_diagnostic() && self.config.output_dir.is_none() {
            return Err(ExploreError::LegacyDiagnosticWithoutOutput);
        }
        journal_authority::admit_root(self.config.output_dir.as_deref()).map_err(|error| {
            ExploreError::Config {
                message: error.message().to_owned(),
            }
        })?;
        let output_dir =
            std::path::PathBuf::from(self.config.output_dir.clone().unwrap_or_default());
        self.open_frontier(&output_dir)?;
        let run_start = Instant::now();

        info!(
            "Starting exploration: {} rounds, {} branches/round, {} VMs, schedule_diversity={}, frontier_policy={}",
            self.config.max_rounds,
            self.config.branch_factor,
            self.config.num_vms,
            self.config.schedule_diversity,
            self.config.frontier_policy
        );

        // Bootstrap: boot kernel + guest init until setup_complete.
        // This uses a generous tick budget (bootstrap_budget) because
        // kernel boot time is variable and much larger than exploration
        // branch budgets.
        info!("Bootstrap: booting kernel + guest init...");
        let initial_result = self.bootstrap()?;

        // Emit Started event for dashboard.
        let catalog_size = self
            .controller
            .as_ref()
            .map(|c| {
                (0..c.num_vms())
                    .map(|i| c.vm(i).fault_engine().oracle().report().catalog_size)
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        let mode_str = match self.config.exploration_mode {
            ExplorationMode::FaultSchedule => "fault-schedule",
            ExplorationMode::InputTree => "input-tree",
            ExplorationMode::Hybrid => "hybrid",
        };
        self.emit_event(crate::dashboard_types::DashboardEvent::Started {
            num_vms: self.config.num_vms,
            seed: self.config.seed,
            branch_factor: self.config.branch_factor,
            ticks_per_branch: self.config.ticks_per_branch,
            max_rounds: self.config.max_rounds,
            mode: mode_str.to_string(),
            kernel_path: self.config.kernel_path.clone(),
            catalog_size,
            from_seed: None,
        });

        // Materialize the helical scenario schedule (if configured).
        let initial_schedule = if let Some(ref scenario_cfg) = self.config.scenario {
            let materialized =
                chaoscontrol_fault::scenario::materialize(scenario_cfg, self.config.seed);
            info!(
                "Materialized {} scenario: {} faults, {} phases",
                scenario_cfg.family,
                materialized.schedule.total(),
                materialized.summary.phases.len(),
            );
            self.scenario_summary = Some(materialized.summary);
            materialized.schedule
        } else {
            FaultSchedule::new()
        };

        self.recovery = initial_result
            .snapshot
            .as_ref()
            .map(|snapshot| RecoverySnapshot::new(snapshot, self.memory_bases.clone()));
        self.admit_frontier_root(initial_result, initial_schedule)?;

        // Create worker pool for parallel execution if configured.
        let effective_workers = match self.config.num_workers {
            0 => {
                // Auto-detect: cores / VMs, capped at branch_factor
                let cores = std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(1);
                let auto = (cores / self.config.num_vms.max(1))
                    .max(1)
                    .min(self.config.branch_factor);
                info!(
                    "Auto-detected {} workers ({} cores, {} VMs)",
                    auto, cores, self.config.num_vms
                );
                auto
            }
            n => n,
        };

        if effective_workers > 1 {
            info!("Creating worker pool with {} workers...", effective_workers);
            match WorkerPool::new(&self.config, effective_workers) {
                Ok(mut pool) => {
                    if !self.memory_bases.is_empty() {
                        pool.set_memory_bases(self.memory_bases.clone());
                    }
                    if let Some(recovery) = &self.recovery {
                        pool.set_recovery(recovery.clone());
                    }
                    self.worker_pool = Some(pool);
                }
                Err(e) => {
                    warn!(
                        "Failed to create worker pool: {}. Falling back to sequential.",
                        e
                    );
                }
            }
        }

        // Main exploration loop
        for round in 0..self.config.max_rounds {
            info!("=== Round {}/{} ===", round + 1, self.config.max_rounds);

            let round_start = Instant::now();
            let round_report = match self.run_round(round)? {
                RoundRun::Completed(report) => report,
                RoundRun::Stopped(stop) => {
                    info!(
                        "Frontier policy stopped before round {}: {}",
                        round + 1,
                        stop
                    );
                    self.conclude(stop)?;
                    break;
                }
            };
            let round_elapsed = round_start.elapsed().as_secs_f64();
            self.rounds_completed += 1;

            // Record per-round history
            let history_entry = RoundHistory {
                round: self.rounds_completed,
                branches_run: round_report.branches_run,
                new_edges: round_report.new_coverage_edges,
                cumulative_edges: self.coverage.stats().total_edges,
                bugs_found: round_report.bugs_found,
                cumulative_bugs: self.all_bugs().count(),
                frontier_size: round_report.frontier_size,
                corpus_size: self.corpus.stats().total_entries,
                restore_ms: round_report.timings.restore_ms,
                run_ms: round_report.timings.run_ms,
                snapshot_ms: round_report.timings.snapshot_ms,
                coverage_ms: round_report.timings.coverage_ms,
                wall_clock_seconds: round_elapsed,
                host_watchdog_timeouts: round_report.watchdog.watchdog_firings,
                host_watchdog_discarded_branches: round_report.watchdog.discarded_branches,
            };
            self.round_history.push(history_entry.clone());
            self.emit_metrics_line(&MetricsLine::from_history(&history_entry));

            // Emit RoundComplete event for dashboard.
            self.emit_event(crate::dashboard_types::DashboardEvent::RoundComplete {
                round: self.rounds_completed,
                branches_run: round_report.branches_run,
                new_edges: round_report.new_coverage_edges,
                cumulative_edges: history_entry.cumulative_edges,
                bugs_found: round_report.bugs_found,
                cumulative_bugs: history_entry.cumulative_bugs,
                frontier_size: round_report.frontier_size,
                corpus_size: history_entry.corpus_size,
                assertion_stats: self.collect_assertion_detail().0,
                wall_clock_seconds: round_elapsed,
                from_seed: None,
            });

            info!(
                "Round {}: {} branches, {} new edges, {} bugs, frontier: {}",
                round + 1,
                round_report.branches_run,
                round_report.new_coverage_edges,
                round_report.bugs_found,
                round_report.frontier_size
            );
            if round_report.watchdog.watchdog_firings > 0 {
                warn!(
                    "Round {}: host watchdog fired {} times, {} branches discarded; operational timeouts, not guest verdicts",
                    round + 1,
                    round_report.watchdog.watchdog_firings,
                    round_report.watchdog.discarded_branches
                );
            }

            // A configured checkpoint is authority-bearing output. Failure is fatal.
            // The Campaign settlement segment was committed by `finish_round`, so the
            // checkpoint never runs ahead of the journal.
            if let Some(ref output_dir) = self.config.output_dir {
                self.save_checkpoint_to_dir(output_dir)?;
            }

            // Track stale rounds (no new edges, no new bugs).
            self.consecutive_stale_rounds = round_stops::next_stale_rounds(
                self.consecutive_stale_rounds,
                round_report.new_coverage_edges,
                round_report.bugs_found,
            );

            // Legacy tries recycling from corpus before giving up. The
            // Campaign policy never recycles: roots exist only at admission.
            let frontier_exhausted = if self.frontier.is_empty() {
                let recycled = if matches!(self.frontier, ExplorerFrontier::Legacy(_)) {
                    self.recycle_frontier_from_corpus()
                } else {
                    0
                };
                if recycled > 0 {
                    info!(
                        "Frontier exhausted — recycled {} entries from corpus",
                        recycled
                    );
                }
                recycled == 0
            } else {
                false
            };

            // Check for stopping conditions, in legacy order.
            let stop = round_stops::post_round_stop(round_stops::PostRoundFacts {
                frontier_exhausted,
                consecutive_stale_rounds: self.consecutive_stale_rounds,
                stale_round_limit: self.config.stale_round_limit,
                round_findings: round_report.bugs_found,
                max_rounds: self.config.max_rounds,
                // Graceful shutdown (Ctrl-C / SIGTERM).
                shutdown_requested: crate::signal::shutdown_requested(),
            });
            if let Some(stop) = stop {
                match stop {
                    ExplorationStop::FrontierExhausted => {
                        info!("Frontier exhausted and corpus empty, stopping");
                    }
                    ExplorationStop::CoveragePlateau => info!(
                        "Coverage plateau: {} consecutive rounds with no new edges or bugs, stopping",
                        self.consecutive_stale_rounds
                    ),
                    ExplorationStop::BugFound => info!("Bug found in short run, stopping"),
                    ExplorationStop::Interrupted => {
                        info!("Shutdown requested, stopping after round {}", round + 1);
                    }
                    ExplorationStop::Completed => {}
                    ExplorationStop::ControlCapacityExhausted => {
                        info!("Campaign control capacity exhausted, stopping");
                    }
                }
                self.conclude(stop)?;
                break;
            }
        }

        // Emit Finished if we completed all rounds (no early break).
        if round_stops::completed_every_round(self.rounds_completed, self.config.max_rounds) {
            self.conclude(ExplorationStop::Completed)?;
        }

        // Generate final report
        let mut report = self.generate_report();
        report.wall_clock_seconds = run_start.elapsed().as_secs_f64();
        if report.wall_clock_seconds > 0.0 {
            report.branches_per_second = report.total_branches as f64 / report.wall_clock_seconds;
            report.edges_per_second = report.total_edges as f64 / report.wall_clock_seconds;
        }
        Ok(report)
    }

    /// Opens the frontier backend and the frontier trace before any KVM work.
    ///
    /// The Campaign policy admits its configuration (degenerate legacy
    /// configurations are rejected), then opens, recovers, and advances the
    /// durable journal under `output_dir`.
    fn open_frontier(&mut self, output_dir: &std::path::Path) -> Result<(), ExploreError> {
        std::fs::create_dir_all(output_dir).map_err(|source| ExploreError::Trace { source })?;
        let journal = match self.config.frontier_policy {
            FrontierPolicy::LegacyDiagnostic => {
                announce_legacy_diagnostic(&self.config, output_dir)?;
                None
            }
            FrontierPolicy::Campaign => {
                let frontier = CampaignFrontier::open(
                    &self.config,
                    output_dir,
                    self.resumed_journal.as_ref(),
                )?;
                let recovery = frontier.recovery();
                if recovery.checkpoint_binding == CheckpointBinding::Mismatched {
                    warn!(
                        "checkpoint journal binding does not match the recovered journal; the journal wins"
                    );
                }
                let facts = TraceJournal {
                    epoch: frontier.epoch(),
                    recovered_epochs: recovery.recovered_epochs,
                    closed_selections: recovery.closed_selections,
                };
                self.frontier = ExplorerFrontier::Campaign(Box::new(frontier));
                Some(facts)
            }
        };
        let mut trace =
            FrontierTrace::open(output_dir).map_err(|source| ExploreError::Trace { source })?;
        trace
            .write(&TraceRecord::header(&self.config, journal))
            .map_err(|source| ExploreError::Trace { source })?;
        self.trace = Some(trace);
        Ok(())
    }

    /// Offers the bootstrap result to the frontier as its root.
    fn admit_frontier_root(
        &mut self,
        initial_result: BranchResult,
        initial_schedule: FaultSchedule,
    ) -> Result<(), ExploreError> {
        let proposals = match initial_result.snapshot {
            Some(_) => self.entry_proposals(&initial_result, 0)?,
            None => Vec::new(),
        };
        let trace_proposals = frontier_backend::trace::trace_proposals(&proposals);
        let digests = self.branch_digests(&initial_result, &initial_schedule)?;
        let base_lengths: Vec<usize> = self.memory_bases.iter().map(|base| base.len()).collect();
        let base_set_id = match &digests {
            Some(digests) => base_set_id(
                self.frontier.epoch(),
                digests.total_ticks,
                &digests.coverage,
                &base_lengths,
            )?,
            None => [0; 32],
        };
        let root = RootAdmission {
            snapshot: initial_result.snapshot,
            coverage: initial_result.coverage,
            schedule: initial_schedule,
            proposals,
            digests,
            base_set_id,
        };
        self.frontier.backend()?.admit_root(root)?;
        let retained = self.frontier.backend()?.retained();
        self.write_trace(&TraceRecord::Root {
            proposals: trace_proposals,
            retained,
        })
    }

    /// Product digests a Campaign moment binds; `None` under the legacy policy.
    fn branch_digests(
        &self,
        result: &BranchResult,
        schedule: &FaultSchedule,
    ) -> Result<Option<BranchDigests>, ExploreError> {
        if self.config.frontier_policy.is_legacy_diagnostic() {
            return Ok(None);
        }
        Ok(Some(BranchDigests {
            schedule: schedule_digest(schedule)?,
            variant: variant_digest(result.schedule_variant.as_ref())?,
            coverage: coverage_digest(&result.coverage),
            oracle: oracle_digest(&result.oracle_report)?,
            total_ticks: result.total_ticks,
        }))
    }

    fn write_trace(&mut self, record: &TraceRecord) -> Result<(), ExploreError> {
        match self.trace.as_mut() {
            Some(trace) => trace
                .write(record)
                .map_err(|source| ExploreError::Trace { source }),
            None => Ok(()),
        }
    }

    /// Records the run's stop: the first stop is published to the frontier
    /// backend and the trace; every stop keeps the legacy dashboard emission.
    fn conclude(&mut self, stop: ExplorationStop) -> Result<(), ExploreError> {
        let first = self.stop.is_none();
        self.finish(stop);
        if !first {
            return Ok(());
        }
        self.frontier.backend()?.record_stop(product_stop(stop))?;
        let record = TraceRecord::Stop {
            stop,
            rounds_completed: self.rounds_completed,
        };
        match self.trace.as_mut() {
            Some(trace) => trace
                .finish(&record)
                .map_err(|source| ExploreError::Trace { source }),
            None => Ok(()),
        }
    }

    // r[impl chaoscontrol.campaign_policy.effects]
    /// One round: begin (draws + plan), authorize (publication fence), execute
    /// (product effects), finish (settlement).
    fn run_round(&mut self, round: u64) -> Result<RoundRun, ExploreError> {
        let mode = RoundMode::for_round(self.config.exploration_mode, round);
        let context = RoundContext { round, mode };
        let planned = match self
            .frontier
            .backend()?
            .begin_round(&mut self.rng, context)?
        {
            RoundBegin::Planned(planned) => planned,
            RoundBegin::Stop(stop) => return Ok(RoundRun::Stopped(exploration_stop(stop))),
        };
        let authorized = match self.frontier.backend()?.authorize(planned)? {
            Authorization::Execute(authorized) => *authorized,
            Authorization::Skipped(skipped) => {
                warn!(
                    "Round {}: selected entry {} has no restorable snapshot; closed unobserved",
                    round + 1,
                    skipped.seq
                );
                let retained = self.frontier.backend()?.retained();
                self.write_trace(&TraceRecord::skipped(&skipped, retained))?;
                return Ok(RoundRun::Completed(RoundReport {
                    branches_run: 0,
                    new_coverage_edges: 0,
                    bugs_found: 0,
                    frontier_size: self.frontier.len(),
                    timings: BranchTimings::default(),
                    watchdog: RoundWatchdogLedger::default(),
                }));
            }
        };
        let (mut report, observation) = match mode {
            RoundMode::FaultSchedule => self.explore_round(&authorized)?,
            RoundMode::InputTree => self.explore_input_tree_round(&authorized)?,
        };
        let context = authorized.context();
        let draws = authorized.draws();
        let work = authorized.work();
        let selected = authorized.selected().cloned();
        let settlement = self
            .frontier
            .backend()?
            .finish_round(authorized, &observation)?;
        report.frontier_size = self.frontier.len();
        let retained = self.frontier.backend()?.retained();
        self.write_trace(&TraceRecord::executed(
            context,
            draws,
            work,
            selected.as_ref(),
            &observation,
            &settlement,
            retained,
        ))?;
        Ok(RoundRun::Completed(report))
    }

    // r[impl chaoscontrol.campaign_policy.product_authority]
    // r[impl chaoscontrol.event_branching.frontier_entry]
    /// Frontier proposals of one branch (one per logical marker/owner, else one).
    ///
    /// Scores against the current global coverage and advances marker hit
    /// counts, so it runs at branch time, exactly where the legacy loop pushed.
    fn entry_proposals(
        &mut self,
        result: &BranchResult,
        depth: u32,
    ) -> Result<Vec<EntryProposal>, ExploreError> {
        let base_score = self.score_branch(result, depth);
        let markers =
            marker_observations(&result.oracle_report).map_err(|error| ExploreError::Config {
                message: format!("invalid branch marker: {error:?}"),
            })?;
        if markers.is_empty() {
            return Ok(vec![EntryProposal {
                score: base_score,
                marker: None,
            }]);
        }
        let mut seen = BTreeSet::new();
        let mut proposals = Vec::new();
        for observation in &markers {
            let prior_hits = update_hit_counts(&mut self.marker_hits, &observation.marker.identity);
            // Every proposal restores the same completed branch snapshot. The
            // first observation has the highest novelty/rarity score; later
            // instances still count as hits, but must not crowd out new parents.
            if seen.insert((
                observation.marker.identity.as_str(),
                observation.marker.owner.as_str(),
            )) {
                proposals.push(EntryProposal {
                    score: marker_score(base_score, prior_hits),
                    marker: Some(frontier_metadata(observation, result.total_ticks)),
                });
            }
        }
        Ok(proposals)
    }

    fn should_retain_branch(
        &self,
        result: &BranchResult,
        new_edges: usize,
    ) -> Result<bool, ExploreError> {
        let markers =
            marker_observations(&result.oracle_report).map_err(|error| ExploreError::Config {
                message: format!("invalid branch marker: {error:?}"),
            })?;
        Ok(new_edges > 0 || !markers.is_empty())
    }

    /// Product facts of one executed branch; retains it when it has new edges
    /// and a snapshot.
    fn branch_outcome(
        &mut self,
        slot: BranchSlot,
        new_edges: usize,
        bugs: &[BugReport],
        enriched: Option<(&BranchResult, &FaultSchedule, u32)>,
        snapshot_present: bool,
    ) -> Result<BranchOutcome, ExploreError> {
        let findings = bugs
            .iter()
            .map(|bug| FindingFacts {
                dedup_key: bug.dedup_key,
                assertion_fingerprint: bug.assertion_identity.fingerprint.0,
            })
            .collect();
        let retained = match enriched {
            Some((result, schedule, depth)) => match result.snapshot.clone() {
                Some(snapshot) => {
                    let proposals = self.entry_proposals(result, depth)?;
                    Some(RetainedBranch {
                        snapshot,
                        coverage: result.coverage.clone(),
                        schedule: schedule.clone(),
                        depth,
                        proposals,
                        digests: self.branch_digests(result, schedule)?,
                    })
                }
                None => None,
            },
            None => None,
        };
        Ok(BranchOutcome {
            slot,
            new_edges,
            snapshot_present,
            findings,
            retained,
        })
    }

    /// Execute one authorized exploration round:
    /// 1. Generate N variant fault schedules from the selected entry
    /// 2. For each variant: restore snapshot → apply schedule → run → collect coverage
    /// 3. Score results; collect frontier proposals and add interesting ones to the corpus
    /// 4. Record any bugs found
    fn explore_round(
        &mut self,
        authorized: &AuthorizedRound,
    ) -> Result<(RoundReport, RoundObservation), ExploreError> {
        let (snapshot, base_schedule, parent_depth) = match authorized.selected() {
            Some(entry) => (
                Some(Arc::new(entry.snapshot.clone())),
                entry.schedule.clone(),
                entry.depth,
            ),
            // Legacy only: no frontier entry, use clean slate.
            None => (None, FaultSchedule::new(), 0),
        };

        // Generate variant schedules. Pre-compute all variants before
        // dispatch so the RNG state advances identically regardless of
        // whether branches run sequentially or in parallel.
        //
        // Switch to havoc mutations when stale rounds are accumulating.
        // Havoc applies 4–16 mutations per variant instead of 1–3, which
        // can break out of local coverage basins.
        let havoc_threshold = if self.config.havoc_after_stale > 0 {
            self.config.havoc_after_stale
        } else {
            self.config.stale_round_limit / 2
        };
        let use_havoc = havoc_threshold > 0 && self.consecutive_stale_rounds >= havoc_threshold;

        let schedule_diversity = schedule_diversity_enabled(
            self.config.schedule_diversity,
            self.config.vm_config.num_vcpus,
        );
        let variants = if use_havoc {
            debug!(
                "Using havoc mutations ({} consecutive stale rounds)",
                self.consecutive_stale_rounds
            );
            if schedule_diversity {
                self.mutator.mutate_havoc_with_schedule(
                    &base_schedule,
                    self.config.branch_factor,
                    &self.config.mutation,
                    self.config.havoc_mutations,
                )
            } else {
                self.mutator
                    .mutate_havoc(
                        &base_schedule,
                        self.config.branch_factor,
                        &self.config.mutation,
                        self.config.havoc_mutations,
                    )
                    .into_iter()
                    .map(|schedule| (schedule, None))
                    .collect()
            }
        } else if schedule_diversity {
            self.mutator.mutate_with_schedule(
                &base_schedule,
                self.config.branch_factor,
                &self.config.mutation,
            )
        } else {
            self.mutator
                .mutate(
                    &base_schedule,
                    self.config.branch_factor,
                    &self.config.mutation,
                )
                .into_iter()
                .map(|schedule| (schedule, None))
                .collect()
        };

        debug!("Generated {} variant schedules", variants.len());

        // Execute branches — parallel if pool available, sequential otherwise.
        let outcomes: Vec<(RecoveredBranchOutcome, FaultSchedule)> =
            match (&mut self.worker_pool, &snapshot) {
                (Some(pool), Some(snap)) => {
                    let work = branch_work_from_variants(&variants);

                    let branch_outcomes = pool.run_branches(snap, work)?;
                    branch_outcomes
                        .into_iter()
                        .zip(variants.into_iter().map(|(schedule, _variant)| schedule))
                        .collect()
                }
                _ => self.run_branches_sequential(&snapshot, variants)?,
            };

        // Discarded branches contribute no coverage, bugs, or frontier entries.
        let mut watchdog = RoundWatchdogLedger::default();
        let results: Vec<(usize, BranchResult, FaultSchedule)> = outcomes
            .into_iter()
            .enumerate()
            .filter_map(|(index, (outcome, schedule))| {
                watchdog
                    .admit(outcome)
                    .map(|result| (index, result, schedule))
            })
            .collect();
        watchdog.ensure_admissible()?;

        // Process results in deterministic order (by branch index).
        let timings = Self::phase_totals_for(&results);
        let mut branches_run = 0;
        let mut new_coverage_edges = 0;
        let mut bugs_found: usize = 0;
        let mut observation = RoundObservation::default();

        for (i, result, schedule) in results {
            branches_run += 1;
            self.total_branches_run += 1;
            let slot = BranchSlot::Variant(branch_slot_index(i)?);

            // Enrich coverage with assertion-state edges so branches
            // with new assertion patterns are "interesting" even after
            // code coverage saturates.
            let mut enriched = result.coverage.clone();
            Self::enrich_with_assertion_state(&mut enriched, &result.oracle_report);
            Self::enrich_with_protocol_events(&mut enriched, &result.oracle_report);
            Self::enrich_with_schedule_fingerprint(&mut enriched, result.schedule_fingerprint);

            let new_edges = enriched.has_new_coverage(self.coverage.global_coverage());
            let branch_bugs = self.extract_bugs(
                &result,
                &schedule,
                snapshot.as_ref(),
                parent_depth,
                authorized
                    .selected()
                    .and_then(|entry| entry.marker.as_ref()),
            )?;
            bugs_found =
                bugs_found
                    .checked_add(branch_bugs.len())
                    .ok_or_else(|| ExploreError::Config {
                        message: "round bug count overflow".to_string(),
                    })?;
            if !branch_bugs.is_empty() {
                warn!("Branch {} found {} bugs!", i + 1, branch_bugs.len());
            }

            let outcome = if self.should_retain_branch(&result, new_edges)? {
                debug!("Branch {} found {} new edges", i + 1, new_edges);
                new_coverage_edges += new_edges;

                let mut enriched_result = result.clone();
                enriched_result.coverage = enriched.clone();

                let outcome = self.branch_outcome(
                    slot,
                    new_edges,
                    &branch_bugs,
                    Some((&enriched_result, &schedule, parent_depth + 1)),
                    result.snapshot.is_some(),
                )?;
                self.add_to_corpus(
                    enriched_result,
                    schedule.clone(),
                    new_edges,
                    branch_bugs,
                    parent_depth + 1,
                );
                outcome
            } else {
                let outcome = self.branch_outcome(
                    slot,
                    new_edges,
                    &branch_bugs,
                    None,
                    result.snapshot.is_some(),
                )?;
                self.retain_standalone_bugs(branch_bugs)?;
                outcome
            };
            observation.branches.push(outcome);

            self.coverage.update_global(&enriched);
        }

        Ok((
            RoundReport {
                branches_run,
                new_coverage_edges,
                bugs_found,
                frontier_size: 0,
                timings,
                watchdog,
            },
            observation,
        ))
    }

    /// Run branches sequentially (original path).
    fn run_branches_sequential(
        &mut self,
        snapshot: &Option<Arc<SimulationSnapshot>>,
        variants: Vec<(FaultSchedule, Option<ScheduleVariant>)>,
    ) -> Result<Vec<(RecoveredBranchOutcome, FaultSchedule)>, ExploreError> {
        let mut outcomes = Vec::with_capacity(variants.len());
        for (i, (schedule, schedule_variant)) in variants.into_iter().enumerate() {
            debug!("Running branch {}/{}", i + 1, self.config.branch_factor);
            let outcome =
                self.run_branch_recovering(snapshot, &schedule, schedule_variant.as_ref(), None)?;
            outcomes.push((outcome, schedule));
        }
        Ok(outcomes)
    }

    /// Execute one authorized input tree exploration round:
    ///
    /// 1. Run a "probe" branch from the selected snapshot — recording all
    ///    random choices the guest makes
    /// 2. Examine the choice history — select interesting decision points
    /// 3. For each selected choice point, restore the snapshot, override
    ///    that choice to an alternate value, and re-run
    /// 4. Score results; collect frontier proposals and add interesting ones to the corpus
    fn explore_input_tree_round(
        &mut self,
        authorized: &AuthorizedRound,
    ) -> Result<(RoundReport, RoundObservation), ExploreError> {
        let mut branches_run = 0;
        let mut new_coverage_edges = 0;
        let mut bugs_found: usize = 0;
        let mut timings = BranchTimings::default();
        let mut observation = RoundObservation::default();
        let mut watchdog = RoundWatchdogLedger::default();

        let (snapshot, base_schedule, parent_depth) = match authorized.selected() {
            Some(entry) => (
                Some(Arc::new(entry.snapshot.clone())),
                entry.schedule.clone(),
                entry.depth,
            ),
            // Legacy only: no frontier entry, use clean slate.
            None => (None, FaultSchedule::new(), 0),
        };

        // Phase 1: Run a "probe" branch to discover choice points.
        // This uses the base schedule (no overrides) — same as the
        // parent run, but we record the choice history.
        let probe_outcome = self.run_branch_recovering(&snapshot, &base_schedule, None, None)?;
        let Some(probe_result) = watchdog.admit(probe_outcome) else {
            // The probe supplies the choice history; without it no alternatives exist.
            watchdog.ensure_admissible()?;
            return Ok((
                RoundReport {
                    branches_run,
                    new_coverage_edges,
                    bugs_found,
                    frontier_size: 0,
                    timings,
                    watchdog,
                },
                observation,
            ));
        };
        timings.restore_ms += probe_result.timings.restore_ms;
        timings.run_ms += probe_result.timings.run_ms;
        timings.snapshot_ms += probe_result.timings.snapshot_ms;
        timings.coverage_ms += probe_result.timings.coverage_ms;
        branches_run += 1;
        self.total_branches_run += 1;
        let probe_bugs = self.extract_bugs(
            &probe_result,
            &base_schedule,
            snapshot.as_ref(),
            parent_depth,
            authorized
                .selected()
                .and_then(|entry| entry.marker.as_ref()),
        )?;
        bugs_found =
            bugs_found
                .checked_add(probe_bugs.len())
                .ok_or_else(|| ExploreError::Config {
                    message: "input-tree bug count overflow".to_string(),
                })?;
        if !probe_bugs.is_empty() {
            warn!("Input tree probe found {} bugs!", probe_bugs.len());
        }

        // Check if the probe itself found new coverage
        let mut probe_enriched = probe_result.coverage.clone();
        Self::enrich_with_assertion_state(&mut probe_enriched, &probe_result.oracle_report);
        Self::enrich_with_protocol_events(&mut probe_enriched, &probe_result.oracle_report);
        Self::enrich_with_schedule_fingerprint(
            &mut probe_enriched,
            probe_result.schedule_fingerprint,
        );
        let probe_new = probe_enriched.has_new_coverage(self.coverage.global_coverage());
        let probe_outcome = if probe_new > 0 {
            new_coverage_edges += probe_new;
            let mut enriched_probe = probe_result.clone();
            enriched_probe.coverage = probe_enriched.clone();
            let outcome = self.branch_outcome(
                BranchSlot::Probe,
                probe_new,
                &probe_bugs,
                Some((&enriched_probe, &base_schedule, parent_depth + 1)),
                probe_result.snapshot.is_some(),
            )?;
            self.add_to_corpus(
                enriched_probe,
                base_schedule.clone(),
                probe_new,
                probe_bugs,
                parent_depth + 1,
            );
            outcome
        } else {
            let outcome = self.branch_outcome(
                BranchSlot::Probe,
                probe_new,
                &probe_bugs,
                None,
                probe_result.snapshot.is_some(),
            )?;
            self.retain_standalone_bugs(probe_bugs)?;
            outcome
        };
        observation.branches.push(probe_outcome);
        self.coverage.update_global(&probe_enriched);

        // Phase 2: Drain choice histories from all VMs.
        self.ensure_controller()?;
        let histories = self.controller.as_mut().unwrap().drain_choice_histories();

        if histories.is_empty() {
            debug!("No random choices recorded — skipping input tree branching");
            return Ok((
                RoundReport {
                    branches_run,
                    new_coverage_edges,
                    bugs_found,
                    frontier_size: 0,
                    timings,
                    watchdog,
                },
                observation,
            ));
        }

        let total_choices: usize = histories.iter().map(|(_, h)| h.len()).sum();
        debug!(
            "Recorded {} choices across {} VMs",
            total_choices,
            histories.len()
        );

        // Phase 3: Select interesting alternatives.
        // Budget = branch_factor - 1 (one branch was used for the probe).
        let alt_budget = self.config.branch_factor.saturating_sub(1);
        let alternatives = input_tree::select_alternatives(&histories, alt_budget, &mut self.rng);

        if alternatives.is_empty() {
            debug!("No viable alternatives found");
            return Ok((
                RoundReport {
                    branches_run,
                    new_coverage_edges,
                    bugs_found,
                    frontier_size: 0,
                    timings,
                    watchdog,
                },
                observation,
            ));
        }

        let num_vms = self.config.num_vms;
        let override_sets = input_tree::alternatives_to_overrides(&alternatives, num_vms);

        info!(
            "Input tree: {} choice points recorded, {} alternatives to explore",
            total_choices,
            override_sets.len()
        );

        // Phase 4: Run each alternative.
        for (i, per_vm_overrides) in override_sets.iter().enumerate() {
            debug!(
                "Running input tree branch {}/{} (choice seq={}, alt={})",
                i + 1,
                override_sets.len(),
                alternatives[i].sequence_id,
                alternatives[i].alternative_value
            );
            let slot = BranchSlot::Alternative(branch_slot_index(i)?);

            let outcome = self.run_branch_recovering(
                &snapshot,
                &base_schedule,
                None,
                Some(per_vm_overrides),
            )?;
            let Some(result) = watchdog.admit(outcome) else {
                debug!("Input tree branch {} discarded after host watchdog", i + 1);
                continue;
            };
            timings.restore_ms += result.timings.restore_ms;
            timings.run_ms += result.timings.run_ms;
            timings.snapshot_ms += result.timings.snapshot_ms;
            timings.coverage_ms += result.timings.coverage_ms;
            branches_run += 1;
            self.total_branches_run += 1;

            // Enrich coverage with assertion-state edges.
            let mut enriched = result.coverage.clone();
            Self::enrich_with_assertion_state(&mut enriched, &result.oracle_report);
            Self::enrich_with_protocol_events(&mut enriched, &result.oracle_report);
            Self::enrich_with_schedule_fingerprint(&mut enriched, result.schedule_fingerprint);

            let new_edges = enriched.has_new_coverage(self.coverage.global_coverage());
            let branch_bugs = self.extract_bugs(
                &result,
                &base_schedule,
                snapshot.as_ref(),
                parent_depth,
                authorized
                    .selected()
                    .and_then(|entry| entry.marker.as_ref()),
            )?;
            bugs_found =
                bugs_found
                    .checked_add(branch_bugs.len())
                    .ok_or_else(|| ExploreError::Config {
                        message: "input-tree bug count overflow".to_string(),
                    })?;
            if !branch_bugs.is_empty() {
                warn!(
                    "Input tree branch {} found {} bugs!",
                    i + 1,
                    branch_bugs.len()
                );
            }

            let outcome = if self.should_retain_branch(&result, new_edges)? {
                debug!("Input tree branch {} found {} new edges", i + 1, new_edges);
                new_coverage_edges += new_edges;

                let mut enriched_result = result.clone();
                enriched_result.coverage = enriched.clone();

                let outcome = self.branch_outcome(
                    slot,
                    new_edges,
                    &branch_bugs,
                    Some((&enriched_result, &base_schedule, parent_depth + 1)),
                    result.snapshot.is_some(),
                )?;
                self.add_to_corpus(
                    enriched_result,
                    base_schedule.clone(),
                    new_edges,
                    branch_bugs,
                    parent_depth + 1,
                );
                outcome
            } else {
                let outcome = self.branch_outcome(
                    slot,
                    new_edges,
                    &branch_bugs,
                    None,
                    result.snapshot.is_some(),
                )?;
                self.retain_standalone_bugs(branch_bugs)?;
                outcome
            };
            observation.branches.push(outcome);

            self.coverage.update_global(&enriched);
        }

        Ok((
            RoundReport {
                branches_run,
                new_coverage_edges,
                bugs_found,
                frontier_size: 0,
                timings,
                watchdog,
            },
            observation,
        ))
    }

    /// Simulation configuration for explorer-owned controllers.
    fn simulation_config(&self) -> SimulationConfig {
        let mut vm_config = self.config.vm_config.clone();
        vm_config.scheduling_strategy = self.config.scheduling_strategy;

        SimulationConfig {
            num_vms: self.config.num_vms,
            vm_config,
            kernel_path: self.config.kernel_path.clone(),
            initrd_path: self.config.initrd_path.clone(),
            seed: self.config.seed,
            quantum: self.config.quantum,
            schedule: FaultSchedule::new(),
            disk_image_path: self.config.disk_image_path.clone(),
            base_core: None,
            bootstrap_budget: Some(self.config.bootstrap_budget),
            dlog_dir: self.config.dlog_dir.clone(),
            random_faults: self.config.random_faults,
        }
    }

    /// Ensure we have a controller ready (created once, reused across branches).
    fn ensure_controller(&mut self) -> Result<(), ExploreError> {
        if self.controller.is_some() {
            return Ok(());
        }

        self.controller = Some(SimulationController::new(self.simulation_config())?);
        Ok(())
    }

    /// Run one branch, replacing the controller after each host watchdog timeout.
    ///
    /// A watchdog timeout permanently poisons the controller, so every firing
    /// installs a replacement before the branch is retried or discarded. Other
    /// errors propagate unchanged.
    fn run_branch_recovering(
        &mut self,
        snapshot: &Option<Arc<SimulationSnapshot>>,
        schedule: &FaultSchedule,
        schedule_variant: Option<&ScheduleVariant>,
        overrides: Option<&[BTreeMap<u64, u64>]>,
    ) -> Result<RecoveredBranchOutcome, ExploreError> {
        let mut watchdog_firings: u32 = 0;
        loop {
            let attempt = match overrides {
                Some(per_vm) => self.run_branch_with_overrides(snapshot, schedule.clone(), per_vm),
                None => self.run_branch(snapshot, schedule.clone(), schedule_variant),
            };
            let error = match attempt {
                Ok(result) => {
                    return Ok(RecoveredBranchOutcome::Completed {
                        result,
                        watchdog_firings,
                    })
                }
                Err(error) => error,
            };
            let Some(vcpu) = explore_error_watchdog_vcpu(&error) else {
                return Err(error);
            };
            let config = self.simulation_config();
            let Some(recovery) = self.recovery.as_ref() else {
                return Err(error);
            };
            watchdog_firings = watchdog_firings.saturating_add(1);
            let decision = decide_after_watchdog(watchdog_firings, snapshot.is_some());
            warn!(
                "Host watchdog interrupted vCPU {} (attempt {}/{}); replacing controller, {:?}",
                vcpu, watchdog_firings, MAX_BRANCH_WATCHDOG_ATTEMPTS, decision
            );
            let replacement = recovery.rebuild_controller(config)?;
            self.controller = Some(replacement);
            if decision == WatchdogDecision::Discard {
                return Ok(RecoveredBranchOutcome::Discarded {
                    vcpu,
                    watchdog_firings,
                });
            }
        }
    }

    /// Bootstrap: boot kernel + run guest until `setup_complete`.
    ///
    /// Returns a `BranchResult` with the initial coverage and a snapshot
    /// taken after setup_complete (or at the budget limit).
    fn bootstrap(&mut self) -> Result<BranchResult, ExploreError> {
        self.ensure_controller()?;

        {
            let controller = self.controller.as_mut().unwrap();
            controller.set_schedule(FaultSchedule::new())?;
            controller.clear_all_coverage();

            // Run until setup_complete or budget exhausted
            controller.run_until_setup_complete(self.config.bootstrap_budget)?;
        }

        // Collect results (same as run_branch phase 2)
        let controller = self.controller.as_ref().unwrap();

        let result_info = controller.report();
        let vm_exit_counts: Vec<u64> = (0..controller.num_vms())
            .map(|i| controller.vm_slot(i).map_or(0, |s| s.vm.exit_count()))
            .collect();
        let total_ticks = controller.tick();

        let _started = Instant::now();
        let coverage = if self.config.coverage_gpa != 0 && controller.num_vms() > 0 {
            if let Some(vm_slot) = controller.vm_slot(0) {
                self.coverage
                    .collect_from_guest(vm_slot.vm.memory().inner())
            } else {
                CoverageBitmap::new()
            }
        } else {
            self.assertion_coverage(&result_info)
        };

        let snap = self.controller.as_mut().unwrap().snapshot_all().ok();

        // Extract base memory images for incremental snapshots.
        // Every subsequent overlay snapshot in this round will share
        // these Arc<Vec<u8>> references instead of copying 256 MB per VM.
        if let Some(ref s) = snap {
            self.memory_bases = SimulationController::extract_memory_bases(s);
        }

        // Set bases on the controller (needs &mut, so reborrow)
        if !self.memory_bases.is_empty() {
            let ctrl_mut = self.controller.as_mut().unwrap();
            ctrl_mut.set_memory_bases(self.memory_bases.clone());
            info!(
                "Stored {} base memory images for incremental snapshots",
                self.memory_bases.len()
            );
        }

        info!(
            "Bootstrap: {} ticks, {} coverage edges",
            total_ticks,
            coverage.count_bits()
        );

        Ok(BranchResult {
            coverage,
            oracle_report: result_info,
            schedule: FaultSchedule::new(),
            exit_counts: vm_exit_counts,
            halted: false,
            total_ticks,
            bugs: Vec::new(),
            snapshot: snap,
            schedule_variant: None,
            schedule_fingerprint: 0,
            timings: BranchTimings::default(),
        })
    }

    /// Run a single branch: restore snapshot → clear coverage → apply
    /// schedule → run for N ticks → collect coverage.
    ///
    /// Reuses the cached controller to avoid re-booting the kernel per
    /// branch (5s saved per branch).
    #[cfg_attr(feature = "profiling", tracing::instrument(skip_all))]
    fn run_branch(
        &mut self,
        snapshot: &Option<Arc<SimulationSnapshot>>,
        schedule: FaultSchedule,
        schedule_variant: Option<&ScheduleVariant>,
    ) -> Result<BranchResult, ExploreError> {
        self.ensure_controller()?;
        let mut timings = BranchTimings::default();

        // Phase 1: restore + run (needs &mut controller)
        {
            let controller = self.controller.as_mut().unwrap();

            // Restore from snapshot if provided (rewinds VM state without reboot)
            if let Some(snap) = snapshot {
                let started = Instant::now();
                if !self.memory_bases.is_empty() {
                    controller.restore_all_incremental(snap)?;
                } else {
                    controller.restore_all(snap)?;
                }
                timings.restore_ms += started.elapsed().as_secs_f64() * 1000.0;
                // Reset all VMs to Running — snapshots may have been taken
                // after idle detection paused a VM.
                controller.reset_vm_statuses();
            }

            if let Some(variant) = schedule_variant {
                controller.apply_schedule_variant(variant)?;
            }

            // Apply the mutated fault schedule in a new branch run.
            controller.begin_counterfactual_fault_run(schedule.clone())?;

            // Clear coverage bitmaps so we only see edges from THIS branch
            controller.clear_all_coverage();

            // Run for configured ticks
            let started = Instant::now();
            controller.run(self.config.ticks_per_branch)?;
            timings.run_ms += started.elapsed().as_secs_f64() * 1000.0;
        }

        // Phase 2: collect results (reborrow controller as immutable,
        // coverage collector as mutable — no overlapping borrows)
        let controller = self.controller.as_ref().unwrap();

        let result_info = controller.report();
        let vm_exit_counts: Vec<u64> = (0..controller.num_vms())
            .map(|i| controller.vm_slot(i).map_or(0, |s| s.vm.exit_count()))
            .collect();
        let total_ticks = controller.tick();

        // Collect coverage from first VM
        let started = Instant::now();
        let coverage = if self.config.coverage_gpa != 0 && controller.num_vms() > 0 {
            if let Some(vm_slot) = controller.vm_slot(0) {
                self.coverage
                    .collect_from_guest(vm_slot.vm.memory().inner())
            } else {
                CoverageBitmap::new()
            }
        } else {
            self.assertion_coverage(&result_info)
        };
        timings.coverage_ms += started.elapsed().as_secs_f64() * 1000.0;
        let schedule_fingerprint = controller.schedule_fingerprint();

        // Use incremental snapshots when base memory is available.
        // This captures only the dirty pages (~1-5% of memory) instead
        // of copying all 256 MB per VM.
        let started = Instant::now();
        let controller = self.controller.as_mut().unwrap();
        let snap = if !self.memory_bases.is_empty() {
            controller
                .snapshot_all_incremental()
                .ok()
                .map(|(s, dirty)| {
                    debug!("Incremental snapshot: {} dirty pages total", dirty);
                    s
                })
        } else {
            controller.snapshot_all().ok()
        };
        timings.snapshot_ms += started.elapsed().as_secs_f64() * 1000.0;

        Ok(BranchResult {
            coverage,
            oracle_report: result_info,
            schedule,
            exit_counts: vm_exit_counts,
            halted: total_ticks >= self.config.ticks_per_branch,
            total_ticks,
            bugs: Vec::new(),
            snapshot: snap,
            schedule_variant: schedule_variant.cloned(),
            schedule_fingerprint,
            timings,
        })
    }

    /// Run a single branch with choice overrides for input tree exploration.
    ///
    /// Same as `run_branch` but also sets per-VM random choice overrides
    /// before running, and clears them after.
    fn run_branch_with_overrides(
        &mut self,
        snapshot: &Option<Arc<SimulationSnapshot>>,
        schedule: FaultSchedule,
        per_vm_overrides: &[BTreeMap<u64, u64>],
    ) -> Result<BranchResult, ExploreError> {
        self.ensure_controller()?;
        let mut timings = BranchTimings::default();

        // Phase 1: restore + set overrides + run
        {
            let controller = self.controller.as_mut().unwrap();

            if let Some(snap) = snapshot {
                let started = Instant::now();
                if !self.memory_bases.is_empty() {
                    controller.restore_all_incremental(snap)?;
                } else {
                    controller.restore_all(snap)?;
                }
                timings.restore_ms += started.elapsed().as_secs_f64() * 1000.0;
                controller.reset_vm_statuses();
            }

            controller.begin_counterfactual_fault_run(schedule.clone())?;

            // Set per-VM choice overrides
            for (vm_id, vm_overrides) in per_vm_overrides.iter().enumerate() {
                if !vm_overrides.is_empty() {
                    controller.set_choice_overrides(vm_id, vm_overrides.clone());
                }
            }

            controller.clear_all_coverage();
            let started = Instant::now();
            controller.run(self.config.ticks_per_branch)?;
            timings.run_ms += started.elapsed().as_secs_f64() * 1000.0;
        }

        // Clear overrides to prevent leaking into subsequent branches
        self.controller
            .as_mut()
            .unwrap()
            .clear_all_choice_overrides();

        // Phase 2: collect results (same as run_branch)
        let controller = self.controller.as_ref().unwrap();

        let result_info = controller.report();
        let vm_exit_counts: Vec<u64> = (0..controller.num_vms())
            .map(|i| controller.vm_slot(i).map_or(0, |s| s.vm.exit_count()))
            .collect();
        let total_ticks = controller.tick();

        let started = Instant::now();
        let coverage = if self.config.coverage_gpa != 0 && controller.num_vms() > 0 {
            if let Some(vm_slot) = controller.vm_slot(0) {
                self.coverage
                    .collect_from_guest(vm_slot.vm.memory().inner())
            } else {
                CoverageBitmap::new()
            }
        } else {
            self.assertion_coverage(&result_info)
        };
        timings.coverage_ms += started.elapsed().as_secs_f64() * 1000.0;
        let schedule_fingerprint = controller.schedule_fingerprint();

        let started = Instant::now();
        let controller = self.controller.as_mut().unwrap();
        let snap = if !self.memory_bases.is_empty() {
            controller.snapshot_all_incremental().ok().map(|(s, _)| s)
        } else {
            controller.snapshot_all().ok()
        };
        timings.snapshot_ms += started.elapsed().as_secs_f64() * 1000.0;

        Ok(BranchResult {
            coverage,
            oracle_report: result_info,
            schedule,
            exit_counts: vm_exit_counts,
            halted: total_ticks >= self.config.ticks_per_branch,
            total_ticks,
            bugs: Vec::new(),
            snapshot: snap,
            schedule_variant: None,
            schedule_fingerprint,
            timings,
        })
    }

    /// Score a branch result for frontier prioritization.
    ///
    /// Uses rare-edge weighting: branches covering edges that few other
    /// branches also cover get a large bonus. This prevents the frontier
    /// from homogenizing around common paths.
    fn score_branch(&self, result: &BranchResult, parent_depth: u32) -> f64 {
        let new_edges = result
            .coverage
            .has_new_coverage(self.coverage.global_coverage());
        let total_edges = result.coverage.count_bits();

        // Base score: number of new edges
        let mut score = new_edges as f64 * 10.0;

        // Rare-edge bonus: edges hit by very few branches are more valuable
        // than common edges.
        let rare_edges = result.coverage.count_rare_edges(
            self.coverage.global_coverage(),
            self.config.rare_edge_threshold,
        );
        score += rare_edges as f64 * self.config.rare_edge_weight;

        // Bonus for high total coverage
        score += total_edges as f64 * 0.1;

        // Penalty for depth (favor shallower branches)
        let depth_penalty = (parent_depth as f64) * 0.5;
        score = (score - depth_penalty).max(0.1);

        // Bonus for assertion diversity
        let assertion_count = result.oracle_report.catalog_size;
        score += assertion_count as f64;

        score
    }

    /// Compute a dedup key from structured assertion identity and fault types.
    fn compute_dedup_key(
        assertion_fingerprint: chaoscontrol_protocol::identity::AssertionFingerprint,
        schedule: &FaultSchedule,
    ) -> Result<u64, ExploreError> {
        const DEDUP_DOMAIN: &[u8] = b"chaoscontrol.bug-dedup.v1\0";
        const DEDUP_KEY_BYTES: usize = core::mem::size_of::<u64>();
        let mut hasher = blake3::Hasher::new();
        hasher.update(DEDUP_DOMAIN);
        hasher.update(&assertion_fingerprint.0);

        let mut type_names: Vec<&str> = schedule
            .faults()
            .iter()
            .map(|scheduled| scheduled.fault.type_name())
            .collect();
        type_names.sort_unstable();
        type_names.dedup();
        for name in &type_names {
            let length = u64::try_from(name.len()).map_err(|_| ExploreError::Config {
                message: "fault type name length exceeds the dedup encoding".to_string(),
            })?;
            hasher.update(&length.to_le_bytes());
            hasher.update(name.as_bytes());
        }
        let digest = hasher.finalize();
        let mut key = [0_u8; DEDUP_KEY_BYTES];
        key.copy_from_slice(&digest.as_bytes()[..DEDUP_KEY_BYTES]);
        Ok(u64::from_le_bytes(key))
    }

    /// Extract bug reports from a branch result, deduplicating by
    /// (assertion fingerprint, sorted fault type set).
    fn extract_bugs(
        &mut self,
        result: &BranchResult,
        schedule: &FaultSchedule,
        replay_snapshot: Option<&Arc<SimulationSnapshot>>,
        parent_depth: u32,
        marker_context: Option<&crate::marker_branching::MarkerFrontierMetadata>,
    ) -> Result<Vec<BugReport>, ExploreError> {
        let replay_parent_depth = if replay_snapshot.is_some() {
            parent_depth
                .checked_add(1)
                .ok_or_else(|| ExploreError::Config {
                    message: "replay parent depth overflow".to_string(),
                })?
        } else {
            0
        };
        if let Some(marker) = marker_context {
            marker.validate().map_err(|error| ExploreError::Config {
                message: format!("invalid parent marker: {error:?}"),
            })?;
            if replay_snapshot.is_none_or(|snapshot| snapshot.tick != marker.observed_tick) {
                return Err(ExploreError::Config {
                    message: "parent marker tick differs from its snapshot".to_string(),
                });
            }
        }
        let mut bugs = Vec::new();
        let has_failed_assertion =
            result
                .oracle_report
                .structured_assertions
                .values()
                .any(|record| {
                    matches!(
                        record.verdict(),
                        chaoscontrol_fault::oracle::Verdict::Failed
                    )
                });
        if !has_failed_assertion {
            return Ok(bugs);
        }
        let report_facts = chaoscontrol_fault::oracle_validation::validate_oracle_report_claim(
            &result.oracle_report,
        )
        .map_err(|error| ExploreError::Config {
            message: format!("failed assertion report is not admissible: {error:?}"),
        })?;

        for (assertion_fingerprint, record) in &result.oracle_report.structured_assertions {
            if matches!(
                record.verdict(),
                chaoscontrol_fault::oracle::Verdict::Failed
            ) {
                let assertion_id = record.compatibility_id.unwrap_or_default();
                let admitted = record
                    .identity
                    .as_ref()
                    .ok_or_else(|| ExploreError::Config {
                        message: "failed assertion has no admitted identity".to_string(),
                    })?;
                let assertion_identity =
                    chaoscontrol_protocol::admission::AssertionEvidenceIdentity::from_admitted(
                        admitted,
                        report_facts.catalog_token,
                    )
                    .map_err(|error| ExploreError::Config {
                        message: format!("failed assertion identity is invalid: {error:?}"),
                    })?;
                if assertion_identity.fingerprint != *assertion_fingerprint {
                    return Err(ExploreError::Config {
                        message: "failed assertion fingerprint does not match its report key"
                            .to_string(),
                    });
                }
                let mut dedup_key = Self::compute_dedup_key(*assertion_fingerprint, schedule)?;
                if !record.process_instances.is_empty() {
                    let mut hasher = blake3::Hasher::new();
                    hasher.update(b"chaoscontrol.process-scoped-bug.v1");
                    hasher.update(&dedup_key.to_le_bytes());
                    for process in &record.process_instances {
                        hasher.update(&(process.len() as u64).to_le_bytes());
                        hasher.update(process.as_bytes());
                    }
                    dedup_key = u64::from_le_bytes(
                        hasher.finalize().as_bytes()[..8]
                            .try_into()
                            .expect("digest prefix"),
                    );
                }

                // Do not conflate failures observed under different runtime owner scopes.
                if self.seen_dedup_keys.contains(&dedup_key) {
                    debug!(
                        "Dedup: skipping duplicate bug (assertion={}, dedup_key={:#x})",
                        record.message, dedup_key
                    );
                    continue;
                }
                self.seen_dedup_keys.insert(dedup_key);

                let bug = BugReport {
                    bug_id: 0, // Will be assigned by corpus
                    assertion_id: assertion_id as u64,
                    assertion_identity,
                    fallback_scope: record.fallback_scope.clone(),
                    process_instances: record.process_instances.clone(),
                    marker_context: marker_context.cloned(),
                    assertion_location: record.message.clone(),
                    schedule: schedule.clone(),
                    snapshot: replay_snapshot.cloned(),
                    tick: result.total_ticks,
                    replay_parent_depth,
                    replay_parent_snapshot_ref: None,
                    dedup_key,
                    schedule_variant: result.schedule_variant.clone(),
                    scenario_config: self.config.scenario.clone(),
                    scenario_summary: self.scenario_summary.clone(),
                    random_faults: self.config.random_faults,
                };

                // Emit BugFound event for dashboard.
                self.emit_event(crate::dashboard_types::DashboardEvent::BugFound {
                    bug_index: self.all_bugs().count() + bugs.len(),
                    assertion_id: assertion_id as u64,
                    assertion_message: record.message.clone(),
                    round: self.rounds_completed,
                    tick: result.total_ticks,
                    schedule_length: schedule.total(),
                    from_seed: None,
                });

                bugs.push(bug);
            }
        }

        Ok(bugs)
    }

    /// Recycle corpus entries into the frontier when it empties.
    ///
    /// Selects corpus entries that cover the most rare edges (global
    /// count ≤ 3) and re-adds them to the frontier with fresh scores.
    /// This prevents exploration from halting when all frontier entries
    /// have been exhausted without finding new code edges.
    ///
    /// Returns the number of entries recycled.
    fn recycle_frontier_from_corpus(&mut self) -> usize {
        let entries = self.corpus.entries();
        if entries.is_empty() {
            return 0;
        }

        // Score each corpus entry by rare-edge count
        let threshold = self.config.rare_edge_threshold;
        let mut scored: Vec<(usize, usize)> = entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let rare = entry
                    .coverage
                    .count_rare_edges(self.coverage.global_coverage(), threshold);
                (i, rare)
            })
            .filter(|(_, rare)| *rare > 0)
            .collect();

        // Sort by rare edge count descending
        scored.sort_by_key(|item| std::cmp::Reverse(item.1));

        // Take up to max_frontier / 2 entries
        let recycle_count = scored.len().min(self.config.max_frontier / 2).max(1);
        let mut recycled = 0;

        for &(idx, rare_count) in scored.iter().take(recycle_count) {
            let entry = &entries[idx];
            // We don't have snapshots in corpus entries, so we can only
            // recycle the schedule. The explorer will re-bootstrap and
            // use the schedule as a base for mutation.
            let frontier_entry = FrontierEntry {
                id: 0,
                snapshot: self
                    .controller
                    .as_mut()
                    .and_then(|c| c.snapshot_all().ok())
                    .unwrap_or_else(|| {
                        // Fallback: empty snapshot (will trigger re-bootstrap)
                        panic!("recycle_frontier_from_corpus: no controller")
                    }),
                coverage: entry.coverage.clone(),
                score: rare_count as f64 * self.config.rare_edge_weight + entry.new_edges as f64,
                times_selected: 0,
                depth: entry.depth,
                schedule: entry.schedule.clone(),
                parent: None,
                marker: None,
            };
            if let ExplorerFrontier::Legacy(frontier) = &mut self.frontier {
                frontier.push_recycled(frontier_entry);
            }
            recycled += 1;
        }

        // If no rare-edge entries, just take the top corpus entries by new_edges
        if recycled == 0 {
            let mut by_edges: Vec<(usize, usize)> = entries
                .iter()
                .enumerate()
                .map(|(i, e)| (i, e.new_edges))
                .collect();
            by_edges.sort_by_key(|item| std::cmp::Reverse(item.1));

            for &(idx, _) in by_edges.iter().take(recycle_count) {
                let entry = &entries[idx];
                if let Some(ref mut controller) = self.controller {
                    if let Ok(snap) = controller.snapshot_all() {
                        let frontier_entry = FrontierEntry {
                            id: 0,
                            snapshot: snap,
                            coverage: entry.coverage.clone(),
                            score: entry.new_edges as f64 * 2.0,
                            times_selected: 0,
                            depth: entry.depth,
                            schedule: entry.schedule.clone(),
                            parent: None,
                            marker: None,
                        };
                        if let ExplorerFrontier::Legacy(frontier) = &mut self.frontier {
                            frontier.push_recycled(frontier_entry);
                        }
                        recycled += 1;
                    }
                }
            }
        }

        recycled
    }

    /// Add a result to the corpus.
    fn add_to_corpus(
        &mut self,
        result: BranchResult,
        schedule: FaultSchedule,
        new_edges: usize,
        bugs: Vec<BugReport>,
        depth: u32,
    ) {
        let entry = CorpusEntry {
            id: 0, // Will be assigned by corpus
            schedule,
            coverage: result.coverage,
            new_edges,
            bugs_found: bugs,
            depth,
        };

        self.corpus.add(entry);
    }

    fn retain_standalone_bugs(&mut self, mut bugs: Vec<BugReport>) -> Result<(), ExploreError> {
        self.corpus
            .assign_bug_ids(&mut bugs)
            .map_err(|message| ExploreError::Config {
                message: message.to_string(),
            })?;
        self.standalone_bugs.extend(bugs);
        Ok(())
    }

    /// Generate pseudo-coverage from assertion variety (blind mode).
    fn assertion_coverage(&self, oracle: &OracleReport) -> CoverageBitmap {
        let mut bitmap = CoverageBitmap::new();

        use std::hash::{Hash, Hasher};
        for (assertion_key, _) in oracle.all_records() {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            assertion_key.hash(&mut hasher);
            let index = hasher.finish() as usize % crate::coverage::MAP_SIZE;
            bitmap.record_hit(index);
        }

        bitmap
    }

    /// Enrich a branch's coverage bitmap with assertion-state edges.
    ///
    /// Hashes each assertion's verdict + hit-count bucket into the coverage
    /// bitmap so that branches with new assertion states are considered
    /// "interesting" even when no new code edges are found. This keeps the
    /// frontier alive after code coverage saturates.
    fn enrich_with_assertion_state(coverage: &mut CoverageBitmap, oracle: &OracleReport) {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        for (assertion_fingerprint, record) in oracle.all_records() {
            // Bucket the hit count: 0, 1, 2-3, 4-7, 8-15, 16-31, 32+
            let hit_bucket = match record.hit_count {
                0 => 0u8,
                1 => 1,
                2..=3 => 2,
                4..=7 => 3,
                8..=15 => 4,
                16..=31 => 5,
                _ => 6,
            };

            // Verdict encoding: 0=unexercised, 1=passed, 2=failed
            let verdict_code = match record.verdict() {
                chaoscontrol_fault::oracle::Verdict::Unexercised => 0u8,
                chaoscontrol_fault::oracle::Verdict::Passed => 1,
                chaoscontrol_fault::oracle::Verdict::Failed => 2,
            };

            // Hash (assertion fingerprint, verdict, hit bucket) into a coverage edge.
            // Use assertion region: [CODE_REGION_END, ASSERTION_REGION_END)
            let assertion_region_size =
                crate::coverage::ASSERTION_REGION_END - crate::coverage::CODE_REGION_END;
            let mut hasher = DefaultHasher::new();
            assertion_fingerprint.hash(&mut hasher);
            verdict_code.hash(&mut hasher);
            hit_bucket.hash(&mut hasher);
            let index = (hasher.finish() as usize % assertion_region_size)
                + crate::coverage::CODE_REGION_END;
            coverage.record_hit(index);

            // Also hash true/false ratio bucket for always/sometimes assertions.
            // This gives finer-grained signal: "50% true" vs "90% true" are different.
            if record.hit_count > 0 {
                const RATIO_BUCKET_COUNT: u128 = 8;
                const RATIO_DOMAIN: u64 = 0xA55E;
                let ratio_bucket = (u128::from(record.true_count) * RATIO_BUCKET_COUNT
                    / u128::from(record.hit_count)) as u8;
                let mut hasher2 = DefaultHasher::new();
                assertion_fingerprint.hash(&mut hasher2);
                RATIO_DOMAIN.hash(&mut hasher2);
                ratio_bucket.hash(&mut hasher2);
                let index2 = (hasher2.finish() as usize % assertion_region_size)
                    + crate::coverage::CODE_REGION_END;
                coverage.record_hit(index2);
            }

            // Hash top-level JSON detail keys from failure details.
            // This distinguishes "election_safety with term=3" from "term=5".
            if let Some(details_bytes) = &record.last_failure_details {
                if let Ok(serde_json::Value::Object(map)) =
                    serde_json::from_slice::<serde_json::Value>(details_bytes)
                {
                    for (key, value) in &map {
                        let mut hasher3 = DefaultHasher::new();
                        format!("assert:{}:{}={}", record.message, key, value).hash(&mut hasher3);
                        let index3 = (hasher3.finish() as usize % assertion_region_size)
                            + crate::coverage::CODE_REGION_END;
                        coverage.record_hit(index3);
                    }
                }
            }
        }
    }

    /// Enrich a branch's coverage bitmap with protocol event edges.
    ///
    /// Hashes each `OracleEvent`'s name and detail key-value pairs into
    /// the assertion region so branches that produce different event
    /// sequences (e.g. leader elected in term 3 vs term 5) look distinct.
    fn enrich_with_protocol_events(coverage: &mut CoverageBitmap, oracle: &OracleReport) {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        if oracle.events.is_empty() {
            return;
        }

        let assertion_region_size =
            crate::coverage::ASSERTION_REGION_END - crate::coverage::CODE_REGION_END;

        for event in &oracle.events {
            // Hash event name
            let mut hasher = DefaultHasher::new();
            format!("event:{}", event.name).hash(&mut hasher);
            let index = (hasher.finish() as usize % assertion_region_size)
                + crate::coverage::CODE_REGION_END;
            coverage.record_hit(index);

            // Hash each top-level detail key-value pair
            if let serde_json::Value::Object(map) = &event.details {
                for (key, value) in map {
                    let mut hasher = DefaultHasher::new();
                    format!("event:{}:{}={}", event.name, key, value).hash(&mut hasher);
                    let index = (hasher.finish() as usize % assertion_region_size)
                        + crate::coverage::CODE_REGION_END;
                    coverage.record_hit(index);
                }
            }
        }
    }

    /// Enrich a branch's coverage bitmap with the schedule fingerprint.
    ///
    /// Hashes the fingerprint into 8 slots in the schedule region
    /// `[ASSERTION_REGION_END, MAP_SIZE)` so branches with different
    /// vCPU interleavings look different to the coverage collector.
    /// No-op when fingerprint is 0 (single-vCPU).
    fn enrich_with_schedule_fingerprint(coverage: &mut CoverageBitmap, fingerprint: u64) {
        if fingerprint == 0 {
            return;
        }

        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let schedule_region_size =
            crate::coverage::MAP_SIZE - crate::coverage::ASSERTION_REGION_END;

        for slot in 0u64..8 {
            let mut hasher = DefaultHasher::new();
            fingerprint.hash(&mut hasher);
            slot.hash(&mut hasher);
            let index = (hasher.finish() as usize % schedule_region_size)
                + crate::coverage::ASSERTION_REGION_END;
            coverage.record_hit(index);
        }
    }

    /// Generate the final exploration report.
    fn generate_report(&self) -> ExplorationReport {
        let bugs = self.all_bugs().cloned().collect();
        let coverage_stats = self.coverage.stats();
        let corpus_stats = self.corpus.stats();

        let network_stats = self
            .controller
            .as_ref()
            .map(|c| c.network().stats().clone())
            .unwrap_or_default();

        // Collect assertion stats, detail, and authority from the merged oracle report.
        let (
            assertion_stats,
            assertion_details,
            assertion_catalog_status,
            collision_safe_assertion_evidence,
            assertion_identity_conflicts,
        ) = self.collect_assertion_detail();

        ExplorationReport {
            rounds: self.rounds_completed,
            total_branches: self.total_branches_run,
            total_edges: coverage_stats.total_edges,
            bugs,
            corpus_size: corpus_stats.total_entries,
            coverage_stats,
            network_stats,
            assertion_stats,
            assertion_details,
            assertion_catalog_status,
            collision_safe_assertion_evidence,
            assertion_identity_conflicts,
            round_history: self.round_history.clone(),
            wall_clock_seconds: 0.0, // Set by caller
            branches_per_second: 0.0,
            edges_per_second: 0.0,
            scenario_config: self.config.scenario.clone(),
            scenario_summary: self.scenario_summary.clone(),
            frontier_policy: self.config.frontier_policy,
            release_eligible: self.release_eligible(),
            stop: self.stop,
        }
    }

    /// Collect assertion stats and per-assertion detail from all VM oracles.
    ///
    /// Reads the merged oracle report and preserves its kind-derived verdicts.
    fn collect_assertion_detail(
        &self,
    ) -> (
        AssertionStats,
        Vec<AssertionDetail>,
        chaoscontrol_protocol::admission::CatalogValidationStatus,
        bool,
        Vec<String>,
    ) {
        use chaoscontrol_fault::oracle::Verdict;

        let Some(controller) = &self.controller else {
            return (
                AssertionStats::default(),
                Vec::new(),
                chaoscontrol_protocol::admission::CatalogValidationStatus::Pending,
                false,
                Vec::new(),
            );
        };
        let report = controller.report();
        let mut passed = 0_usize;
        let mut failed = 0_usize;
        let mut unexercised = 0_usize;
        let mut details = Vec::with_capacity(report.catalog_size);
        for (_, record) in report.all_records() {
            let verdict = record.verdict();
            match verdict {
                Verdict::Passed => passed += 1,
                Verdict::Failed => failed += 1,
                Verdict::Unexercised => unexercised += 1,
            }
            let failure_details = record
                .last_failure_details
                .as_ref()
                .and_then(|bytes| std::str::from_utf8(bytes).ok().map(str::to_string));
            let identity = record.identity.as_ref().map(|admitted| {
                let descriptor = &admitted.descriptor;
                AssertionIdentityDetail {
                    descriptor: descriptor.clone(),
                    fingerprint: admitted.fingerprint,
                    canonical_descriptor: chaoscontrol_protocol::identity::encode_lower_hex(
                        &admitted.canonical_bytes,
                    ),
                    catalog_tokens: record.catalog_tokens.iter().copied().collect(),
                }
            });
            details.push(AssertionDetail {
                id: record.compatibility_id.unwrap_or_default(),
                identity,
                process_instances: record.process_instances.clone(),
                message: record.message.clone(),
                kind: format!("{:?}", record.kind).to_lowercase(),
                guest: record.guest.clone(),
                category: record.category.clone(),
                verdict: format!("{:?}", verdict).to_lowercase(),
                hit_count: record.hit_count,
                true_count: record.true_count,
                false_count: record.false_count,
                last_failure_details: failure_details,
            });
        }
        details.sort_by(|left, right| {
            let order = |verdict: &str| match verdict {
                "failed" => 0_u8,
                "unexercised" => 1_u8,
                _ => 2_u8,
            };
            let left_fingerprint = left.identity.as_ref().map(|identity| identity.fingerprint);
            let right_fingerprint = right.identity.as_ref().map(|identity| identity.fingerprint);
            order(&left.verdict)
                .cmp(&order(&right.verdict))
                .then(left_fingerprint.cmp(&right_fingerprint))
                .then(left.id.cmp(&right.id))
        });
        let stats = AssertionStats {
            catalog_size: details.len(),
            passed,
            failed,
            unexercised,
        };
        (
            stats,
            details,
            report.catalog_status,
            report.collision_safe_evidence,
            report.identity_conflicts,
        )
    }

    /// Get current exploration stats.
    pub fn stats(&self) -> ExplorationStats {
        ExplorationStats {
            rounds: self.rounds_completed,
            branches: self.total_branches_run,
            edges: self.coverage.stats().total_edges,
            bugs: self.all_bugs().count(),
            frontier_size: self.frontier.len(),
            corpus_size: self.corpus.len(),
        }
    }

    /// Get a snapshot of the current exploration state for the dashboard.
    pub fn snapshot_state(&self) -> crate::dashboard_types::DashboardState {
        use crate::dashboard_types::*;

        let (assertion_stats, assertion_details, _, _, _) = self.collect_assertion_detail();

        let bugs: Vec<DashboardBug> = self
            .corpus
            .bugs()
            .iter()
            .map(|b| DashboardBug {
                bug_id: b.bug_id,
                assertion_id: b.assertion_id,
                assertion_message: b.assertion_location.clone(),
                round: 0, // not tracked per-bug currently
                tick: b.tick,
                schedule_length: b.schedule.total(),
            })
            .collect();

        let network_stats = self
            .controller
            .as_ref()
            .map(|c| DashboardNetworkStats::from(c.network().stats()))
            .unwrap_or_default();

        let mode_str = match self.config.exploration_mode {
            ExplorationMode::FaultSchedule => "fault-schedule",
            ExplorationMode::InputTree => "input-tree",
            ExplorationMode::Hybrid => "hybrid",
        };

        DashboardState {
            running: true,
            config: DashboardConfig {
                num_vms: self.config.num_vms,
                seed: self.config.seed,
                branch_factor: self.config.branch_factor,
                ticks_per_branch: self.config.ticks_per_branch,
                max_rounds: self.config.max_rounds,
                mode: mode_str.to_string(),
                kernel_path: self.config.kernel_path.clone(),
            },
            rounds: self.rounds_completed,
            total_branches: self.total_branches_run,
            total_edges: self.coverage.stats().total_edges,
            bugs,
            corpus_size: self.corpus.len(),
            coverage_stats: self.coverage.stats(),
            network_stats,
            assertion_stats,
            assertion_details,
            round_history: self.round_history.clone(),
            finish_reason: String::new(),
            mode: "run".to_string(),
            seeds_total: 0,
            seeds_completed: 0,
            seed_summaries: Vec::new(),
        }
    }

    /// Project declared and reached branch-marker coverage from the live controller.
    pub fn marker_coverage_report(
        &self,
    ) -> Result<
        crate::marker_branching::MarkerCoverageReport,
        crate::marker_branching::MarkerBindingError,
    > {
        let report = self
            .controller
            .as_ref()
            .map_or_else(OracleReport::empty, SimulationController::report);
        crate::marker_branching::oracle_coverage_report(&report)
    }

    /// Get a mutable reference to the config (for runtime adjustments).
    pub fn config_mut(&mut self) -> &mut ExplorerConfig {
        &mut self.config
    }

    /// Save a checkpoint to the specified directory.
    pub fn save_checkpoint_to_dir(&self, dir: &str) -> Result<(), CheckpointError> {
        use std::fs;

        // Create directory if it doesn't exist
        fs::create_dir_all(dir)?;

        let checkpoint_path = format!("{}/checkpoint.json", dir);
        let checkpoint = self.create_checkpoint(dir)?;
        save_checkpoint(&checkpoint_path, &checkpoint)?;

        info!("Checkpoint saved to {}", checkpoint_path);
        Ok(())
    }

    /// Create a checkpoint from the current state.
    fn create_checkpoint(
        &self,
        output_dir: &str,
    ) -> Result<ExplorationCheckpoint, CheckpointError> {
        let config = CheckpointConfig {
            num_vms: self.config.num_vms,
            kernel_path: self.config.kernel_path.clone(),
            initrd_path: self.config.initrd_path.clone(),
            seed: self.config.seed,
            branch_factor: self.config.branch_factor,
            ticks_per_branch: self.config.ticks_per_branch,
            max_rounds: self.config.max_rounds,
            max_frontier: self.config.max_frontier,
            quantum: self.config.quantum,
            coverage_gpa: self.config.coverage_gpa,
            disk_image_path: self.config.disk_image_path.clone(),
            bootstrap_budget: self.config.bootstrap_budget,
            schedule_diversity: self.config.schedule_diversity,
            schedule_mutation_ratio: self.config.mutation.schedule_mutation_ratio,
            rare_edge_threshold: Some(self.config.rare_edge_threshold),
            rare_edge_weight: Some(self.config.rare_edge_weight),
            havoc_after_stale: Some(self.config.havoc_after_stale),
            havoc_mutations: Some(self.config.havoc_mutations),
            scenario: self.config.scenario.clone(),
            frontier_policy: self.config.frontier_policy,
            random_faults: self.config.random_faults,
        };

        let snapshot_store = crate::snapshot_store::FileSnapshotStore::new(output_dir);
        let mut bugs = Vec::new();
        // Memoize only this borrowed checkpoint view. Pointer identity never
        // escapes into an artifact, and depth remains part of the envelope.
        let mut replay_parents =
            BTreeMap::<_, crate::snapshot_store::ReplayParentSnapshotRef>::new();
        for bug in self.all_bugs() {
            let mut serialized: SerializableBug = bug.into();
            if bug.replay_parent_depth > 0 && serialized.replay_parent_snapshot_ref.is_none() {
                let snapshot = bug.snapshot.as_ref().ok_or(
                    CheckpointError::MissingRequiredReplayParentSnapshot {
                        bug_id: bug.bug_id,
                        replay_parent_depth: bug.replay_parent_depth,
                    },
                )?;
                let key = (Arc::as_ptr(snapshot), bug.replay_parent_depth);
                let reference = if let Some(reference) = replay_parents.get(&key) {
                    reference.clone()
                } else {
                    let reference = crate::snapshot_store::SnapshotStore::put_snapshot(
                        &snapshot_store,
                        snapshot,
                        bug.replay_parent_depth,
                    )?;
                    replay_parents.insert(key, reference.clone());
                    reference
                };
                serialized.replay_parent_snapshot_ref = Some(reference);
            }
            if bug.replay_parent_depth == 0 {
                serialized.replay_parent_snapshot_ref = None;
            }
            bugs.push(serialized);
        }

        let assertion_report = if let Some(controller) = self.controller.as_ref() {
            let report = controller.report();
            match chaoscontrol_fault::oracle_validation::validate_oracle_report_claim(&report) {
                Ok(_) => Some(report),
                Err(_) => None,
            }
        } else {
            None
        };
        crate::checkpoint::validate_bug_set(&bugs, assertion_report.as_ref())?;

        Ok(ExplorationCheckpoint {
            config,
            global_coverage: self.coverage.global_coverage().as_slice().to_vec(),
            bugs,
            assertion_report,
            rounds_completed: self.rounds_completed,
            total_branches_run: self.total_branches_run,
            total_edges: self.coverage.stats().total_edges,
            seed: self.config.seed,
            round_history: Some(self.round_history.clone()),
            seen_dedup_keys: Some(self.seen_dedup_keys.iter().copied().collect()),
            scenario: self.config.scenario.clone(),
            scenario_summary: self.scenario_summary.clone(),
            campaign_journal: self.frontier.journal_binding(),
            release_eligible: self.release_eligible(),
        })
    }

    /// Create an Explorer from a checkpoint, optionally overriding config fields.
    pub fn from_checkpoint(
        checkpoint: ExplorationCheckpoint,
        kernel_path_override: Option<String>,
        initrd_path_override: Option<String>,
        max_rounds_override: Option<u64>,
    ) -> Result<Self, crate::checkpoint::BugSetIdentityError> {
        let standalone_bugs = crate::checkpoint::replay_bug_set(
            &checkpoint.bugs,
            checkpoint.assertion_report.as_ref(),
        )?;
        let config = ExplorerConfig {
            num_vms: checkpoint.config.num_vms,
            vm_config: VmConfig::default(),
            kernel_path: kernel_path_override.unwrap_or(checkpoint.config.kernel_path),
            initrd_path: initrd_path_override.or(checkpoint.config.initrd_path),
            seed: checkpoint.config.seed,
            branch_factor: checkpoint.config.branch_factor,
            ticks_per_branch: checkpoint.config.ticks_per_branch,
            max_rounds: max_rounds_override.unwrap_or(checkpoint.config.max_rounds),
            max_frontier: checkpoint.config.max_frontier,
            quantum: checkpoint.config.quantum,
            scheduling_strategy: SchedulingStrategy::RoundRobin,
            mutation: MutationConfig::default(),
            exploration_mode: ExplorationMode::default(),
            coverage_gpa: checkpoint.config.coverage_gpa,
            output_dir: None, // Will be set by caller if needed
            disk_image_path: checkpoint.config.disk_image_path,
            bootstrap_budget: checkpoint.config.bootstrap_budget,
            dlog_dir: None,
            dlog_register_interval: 0,
            dlog_memory_hash: false,
            num_workers: 1,
            stale_round_limit: 10,
            schedule_diversity: checkpoint.config.schedule_diversity,
            rare_edge_threshold: checkpoint.config.rare_edge_threshold.unwrap_or(3),
            rare_edge_weight: checkpoint.config.rare_edge_weight.unwrap_or(5.0),
            havoc_after_stale: checkpoint.config.havoc_after_stale.unwrap_or(0),
            havoc_mutations: checkpoint.config.havoc_mutations.unwrap_or([4, 16]),
            scenario: checkpoint.config.scenario.clone(),
            emit_metrics: false,
            metrics_file: None,
            frontier_policy: checkpoint.config.frontier_policy,
            random_faults: checkpoint.config.random_faults,
        };

        let frontier = ExplorerFrontier::for_config(&config);
        let corpus = Corpus::new();
        let mutator = ScheduleMutator::new(config.seed);

        // Restore global coverage
        let mut coverage = CoverageCollector::new(config.coverage_gpa);
        let restored_bitmap = CoverageBitmap::from_slice(&checkpoint.global_coverage);
        coverage.update_global(&restored_bitmap);

        let rng = ChaCha8Rng::seed_from_u64(config.seed);

        info!(
            "Restored checkpoint: {} rounds completed, {} branches, {} edges",
            checkpoint.rounds_completed, checkpoint.total_branches_run, checkpoint.total_edges
        );

        Ok(Self {
            config,
            frontier,
            corpus,
            mutator,
            coverage,
            rng,
            controller: None,
            rounds_completed: checkpoint.rounds_completed,
            total_branches_run: checkpoint.total_branches_run,
            round_history: checkpoint.round_history.unwrap_or_default(),
            memory_bases: Vec::new(),
            worker_pool: None,
            event_sink: None,
            seen_dedup_keys: checkpoint
                .seen_dedup_keys
                .unwrap_or_default()
                .into_iter()
                .collect(),
            standalone_bugs,
            consecutive_stale_rounds: 0,
            marker_hits: BTreeMap::new(),
            scenario_summary: checkpoint.scenario_summary.clone(),
            metrics_sink: None,
            stop: None,
            trace: None,
            resumed_journal: checkpoint.campaign_journal.clone(),
            inherits_diagnostic_rounds: !checkpoint.release_eligible,
            recovery: None,
        })
    }
}

/// Per-branch wall-clock phase timings.
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct BranchTimings {
    #[serde(default)]
    pub restore_ms: f64,
    #[serde(default)]
    pub run_ms: f64,
    #[serde(default)]
    pub snapshot_ms: f64,
    #[serde(default)]
    pub coverage_ms: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MetricsLine {
    pub round: u64,
    pub branches: usize,
    pub new_edges: usize,
    pub cumulative_edges: usize,
    pub bugs_found: usize,
    pub restore_ms: f64,
    pub run_ms: f64,
    pub snapshot_ms: f64,
    pub coverage_ms: f64,
    pub wall_ms: f64,
    pub host_watchdog_timeouts: usize,
    pub host_watchdog_discarded_branches: usize,
}

impl MetricsLine {
    pub fn from_history(history: &RoundHistory) -> Self {
        Self {
            round: history.round,
            branches: history.branches_run,
            new_edges: history.new_edges,
            cumulative_edges: history.cumulative_edges,
            bugs_found: history.bugs_found,
            restore_ms: history.restore_ms,
            run_ms: history.run_ms,
            snapshot_ms: history.snapshot_ms,
            coverage_ms: history.coverage_ms,
            wall_ms: history.wall_clock_seconds * 1000.0,
            host_watchdog_timeouts: history.host_watchdog_timeouts,
            host_watchdog_discarded_branches: history.host_watchdog_discarded_branches,
        }
    }
}

/// Result of running a single branch.
pub struct BranchResult {
    pub coverage: CoverageBitmap,
    pub oracle_report: OracleReport,
    pub schedule: FaultSchedule,
    pub exit_counts: Vec<u64>,
    pub halted: bool,
    pub total_ticks: u64,
    pub bugs: Vec<BugReport>,
    pub snapshot: Option<chaoscontrol_vmm::controller::SimulationSnapshot>,
    /// Schedule variant used for this branch (None = default scheduling).
    pub schedule_variant: Option<ScheduleVariant>,
    /// Combined schedule fingerprint from all VMs.
    pub schedule_fingerprint: u64,
    /// Per-branch wall-clock phase timings.
    pub timings: BranchTimings,
}

impl Clone for BranchResult {
    fn clone(&self) -> Self {
        Self {
            coverage: self.coverage.clone(),
            oracle_report: self.oracle_report.clone(),
            schedule: self.schedule.clone(),
            exit_counts: self.exit_counts.clone(),
            halted: self.halted,
            total_ticks: self.total_ticks,
            bugs: self.bugs.clone(),
            snapshot: self.snapshot.clone(),
            schedule_variant: self.schedule_variant.clone(),
            schedule_fingerprint: self.schedule_fingerprint,
            timings: self.timings,
        }
    }
}

/// Result of a single exploration round.
#[derive(Debug)]
pub struct RoundReport {
    pub branches_run: usize,
    pub new_coverage_edges: usize,
    pub bugs_found: usize,
    pub frontier_size: usize,
    pub timings: BranchTimings,
    /// Host watchdog accounting for this round.
    pub watchdog: RoundWatchdogLedger,
}

/// Per-round snapshot of exploration progress.
///
/// Captured after each round completes, these records track coverage growth,
/// bug discovery timing, and frontier evolution across the campaign.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoundHistory {
    /// Round number (1-indexed).
    pub round: u64,
    /// Branches executed this round.
    pub branches_run: usize,
    /// New coverage edges discovered this round.
    pub new_edges: usize,
    /// Cumulative unique edges after this round.
    pub cumulative_edges: usize,
    /// Bugs found this round.
    pub bugs_found: usize,
    /// Cumulative bugs after this round.
    pub cumulative_bugs: usize,
    /// Frontier size after this round.
    pub frontier_size: usize,
    /// Corpus size after this round.
    pub corpus_size: usize,
    /// Time spent restoring snapshots during this round.
    #[serde(default)]
    pub restore_ms: f64,
    /// Time spent running VMs during this round.
    #[serde(default)]
    pub run_ms: f64,
    /// Time spent taking snapshots during this round.
    #[serde(default)]
    pub snapshot_ms: f64,
    /// Time spent collecting coverage during this round.
    #[serde(default)]
    pub coverage_ms: f64,
    /// Wall-clock time for this round (seconds). 0.0 for old checkpoints.
    #[serde(default)]
    pub wall_clock_seconds: f64,
    /// Host watchdog firings this round, including firings recovered by retry.
    /// Operational evidence only; 0 for old checkpoints.
    #[serde(default)]
    pub host_watchdog_timeouts: usize,
    /// Branches discarded after exhausting host watchdog attempts this round.
    /// A nonzero value means the exploration trajectory depended on host timing.
    #[serde(default)]
    pub host_watchdog_discarded_branches: usize,
}

/// Final exploration report.
#[derive(Debug, Clone)]
pub struct ExplorationReport {
    pub rounds: u64,
    pub total_branches: u64,
    pub total_edges: usize,
    pub bugs: Vec<BugReport>,
    pub corpus_size: usize,
    pub coverage_stats: CoverageStats,
    pub network_stats: chaoscontrol_vmm::controller::NetworkStats,
    pub assertion_stats: AssertionStats,
    /// Per-assertion detail — individual verdicts, hit counts, messages.
    pub assertion_details: Vec<AssertionDetail>,
    /// Catalog authority retained from the validated merged oracle report.
    pub assertion_catalog_status: chaoscontrol_protocol::admission::CatalogValidationStatus,
    /// True only for a non-empty validated accepted assertion catalog.
    pub collision_safe_assertion_evidence: bool,
    /// Assertion identity diagnostics retained from the merged oracle report.
    pub assertion_identity_conflicts: Vec<String>,
    /// Per-round exploration history.
    pub round_history: Vec<RoundHistory>,
    /// Total wall-clock time for the exploration run.
    pub wall_clock_seconds: f64,
    /// Branch throughput over wall-clock runtime.
    pub branches_per_second: f64,
    /// Coverage throughput over wall-clock runtime.
    pub edges_per_second: f64,
    /// Helical scenario config used (if any).
    pub scenario_config: Option<chaoscontrol_fault::scenario::ScenarioConfig>,
    /// Materialized phase summary (if a scenario was used).
    pub scenario_summary: Option<chaoscontrol_fault::scenario::PhaseSummary>,
    /// Frontier policy that selected every expansion.
    pub frontier_policy: FrontierPolicy,
    /// False for legacy-diagnostic runs and runs resumed from their
    /// checkpoints; `report.txt` then carries the release-ineligible line.
    pub release_eligible: bool,
    /// First stop of the run. Bounded sampling: see [`EXHAUSTIVE_SEARCH_NON_CLAIM`].
    pub stop: Option<ExplorationStop>,
}

/// Summary of assertion coverage across all exploration branches.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AssertionStats {
    /// Total registered assertion sites (catalog + runtime).
    pub catalog_size: usize,
    /// Assertions that passed across all runs.
    pub passed: usize,
    /// Assertions that failed in at least one run.
    pub failed: usize,
    /// Assertions registered but never evaluated.
    pub unexercised: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssertionIdentityDetail {
    pub descriptor: chaoscontrol_protocol::identity::AssertionDescriptor,
    pub fingerprint: chaoscontrol_protocol::identity::AssertionFingerprint,
    pub canonical_descriptor: String,
    pub catalog_tokens: Vec<chaoscontrol_protocol::identity::AssertionFingerprint>,
}

/// Per-assertion detail for the exploration report.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssertionDetail {
    /// Non-authoritative compact alias.
    pub id: u32,
    /// Collision-safe identity. None means diagnostic legacy input.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::non_null_option::deserialize"
    )]
    pub identity: Option<AssertionIdentityDetail>,
    /// Runtime owners observed for this assertion, not whole-guest coverage.
    #[serde(
        default,
        deserialize_with = "chaoscontrol_replay_evidence_core::validate::deserialize_process_instances",
        skip_serializing_if = "std::collections::BTreeSet::is_empty"
    )]
    pub process_instances: std::collections::BTreeSet<String>,
    /// Human-readable message.
    pub message: String,
    /// Assertion kind: "always", "sometimes", "reachable", "unreachable".
    pub kind: String,
    /// Guest name for assertion-density reporting.
    pub guest: String,
    /// Density category for grouped exercise reporting.
    pub category: String,
    /// Final verdict: "passed", "failed", "unexercised".
    pub verdict: String,
    /// Total evaluation count across all runs.
    pub hit_count: u64,
    /// Times condition was true (always/sometimes only).
    pub true_count: u64,
    /// Times condition was false (always/sometimes only).
    pub false_count: u64,
    /// JSON details from the most recent failure (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_failure_details: Option<String>,
}

/// Current exploration statistics.
#[derive(Debug, Clone)]
pub struct ExplorationStats {
    pub rounds: u64,
    pub branches: u64,
    pub edges: usize,
    pub bugs: usize,
    pub frontier_size: usize,
    pub corpus_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_journal_authority_stops_before_bootstrap_and_metrics() {
        let directory = tempfile::tempdir().expect("test directory");
        let metrics_path = directory.path().join("metrics.jsonl");
        for output_dir in [None, Some(String::new()), Some("bad\0root".to_owned())] {
            let mut explorer = Explorer::new(ExplorerConfig {
                output_dir,
                metrics_file: Some(metrics_path.clone()),
                emit_metrics: true,
                ..ExplorerConfig::default()
            });
            let error = explorer
                .run()
                .expect_err("journal authority must precede KVM");
            match error {
                ExploreError::Config { message } => {
                    assert!(message.contains("journal"), "unexpected error: {message}");
                }
                other => panic!("journal admission did not precede bootstrap: {other:?}"),
            }
            assert!(explorer.controller.is_none());
            assert_eq!(explorer.rounds_completed, 0);
            assert_eq!(explorer.total_branches_run, 0);
            assert!(explorer.metrics_sink.is_none());
            assert!(!metrics_path.exists());
        }
    }

    // r[verify chaoscontrol.campaign_policy.adapter.invalid]
    #[test]
    fn campaign_policy_rejects_degenerate_configs_before_kvm() {
        let directory = tempfile::tempdir().expect("test directory");
        let base = ExplorerConfig {
            output_dir: Some(directory.path().display().to_string()),
            ..ExplorerConfig::default()
        };
        assert_eq!(base.frontier_policy, FrontierPolicy::Campaign);
        for config in [
            ExplorerConfig {
                max_frontier: 0,
                ..base.clone()
            },
            ExplorerConfig {
                rare_edge_weight: f64::NAN,
                ..base.clone()
            },
        ] {
            let mut explorer = Explorer::new(config);
            match explorer.run() {
                Err(ExploreError::Frontier {
                    source:
                        FrontierError::Adapter(crate::campaign_adapter::AdapterError::DegenerateConfig(
                            _,
                        )),
                }) => {}
                Err(other) => panic!("unexpected error {other:?}"),
                Ok(_) => panic!("degenerate configuration explored"),
            }
            assert!(explorer.controller.is_none(), "no KVM work started");
            assert!(!directory
                .path()
                .join(journal::JOURNAL_DIRECTORY_NAME)
                .exists());
        }
        // The same configurations keep legacy behavior under the explicit
        // legacy-diagnostic rollback policy.
        let legacy = Explorer::new(ExplorerConfig {
            max_frontier: 0,
            frontier_policy: FrontierPolicy::LegacyDiagnostic,
            ..base
        });
        assert!(matches!(legacy.frontier, ExplorerFrontier::Legacy(_)));
    }

    // r[verify chaoscontrol.campaign_policy.cutover.valid]
    // r[verify chaoscontrol.campaign_policy.rollback.invalid]
    #[test]
    fn default_config_selects_the_campaign_backend() {
        let explorer = Explorer::new(ExplorerConfig::default());
        assert_eq!(explorer.config.frontier_policy, FrontierPolicy::Campaign);
        assert!(matches!(
            explorer.frontier,
            ExplorerFrontier::CampaignUnopened
        ));
        assert!(explorer.release_eligible());
    }

    // r[verify chaoscontrol.campaign_policy.rollback.valid]
    #[test]
    fn legacy_diagnostic_without_output_dir_is_rejected_before_kvm() {
        let mut explorer = Explorer::new(ExplorerConfig {
            frontier_policy: FrontierPolicy::LegacyDiagnostic,
            output_dir: None,
            ..ExplorerConfig::default()
        });
        assert!(matches!(
            explorer.run(),
            Err(ExploreError::LegacyDiagnosticWithoutOutput)
        ));
        assert!(explorer.controller.is_none(), "no KVM work started");
    }

    // r[verify chaoscontrol.campaign_policy.rollback.valid]
    #[test]
    fn only_legacy_diagnostic_runs_write_the_release_ineligible_receipt() {
        use chaoscontrol_evidence::frontier_policy_gate::FRONTIER_POLICY_RECEIPT_FILE;

        let diagnostic_dir = tempfile::tempdir().expect("test directory");
        let mut diagnostic = Explorer::new(ExplorerConfig {
            frontier_policy: FrontierPolicy::LegacyDiagnostic,
            output_dir: Some(diagnostic_dir.path().display().to_string()),
            ..ExplorerConfig::default()
        });
        diagnostic
            .open_frontier(diagnostic_dir.path())
            .expect("diagnostic frontier opens");
        let receipt: FrontierPolicyReceipt = serde_json::from_str(
            &std::fs::read_to_string(diagnostic_dir.path().join(FRONTIER_POLICY_RECEIPT_FILE))
                .expect("diagnostic receipt written before bootstrap"),
        )
        .expect("receipt parses");
        assert_eq!(receipt.frontier_policy, FrontierPolicy::LegacyDiagnostic);
        assert!(!receipt.release_eligible);
        assert!(diagnostic.controller.is_none(), "no KVM work started");

        let campaign_dir = tempfile::tempdir().expect("test directory");
        let mut campaign = Explorer::new(ExplorerConfig {
            output_dir: Some(campaign_dir.path().display().to_string()),
            ..ExplorerConfig::default()
        });
        campaign
            .open_frontier(campaign_dir.path())
            .expect("Campaign frontier opens");
        assert!(!campaign_dir
            .path()
            .join(FRONTIER_POLICY_RECEIPT_FILE)
            .exists());
    }

    // r[verify chaoscontrol.campaign_policy.progress.valid]
    // r[verify chaoscontrol.campaign_policy.rollback.valid]
    #[test]
    fn campaign_checkpoint_keeps_its_shape_and_diagnostic_marks_are_additive() {
        let directory = tempfile::tempdir().expect("test directory");
        let output = directory.path().display().to_string();
        let campaign = Explorer::new(ExplorerConfig::default());
        let checkpoint = campaign
            .create_checkpoint(&output)
            .expect("campaign checkpoint");
        let json = serde_json::to_value(&checkpoint).expect("checkpoint json");
        assert!(json["config"].get("frontier_policy").is_none());
        assert!(json.get("release_eligible").is_none());
        assert!(json.get("campaign_journal").is_none());
        let report = campaign.generate_report();
        assert_eq!(report.frontier_policy, FrontierPolicy::Campaign);
        assert!(report.release_eligible);
        assert_eq!(report.stop, None);

        // The diagnostic policy adds only the policy and eligibility marks;
        // product progress, coverage, and bugs keep their shape.
        let diagnostic = Explorer::new(ExplorerConfig {
            frontier_policy: FrontierPolicy::LegacyDiagnostic,
            ..ExplorerConfig::default()
        });
        let diagnostic_json =
            serde_json::to_value(diagnostic.create_checkpoint(&output).expect("checkpoint"))
                .expect("checkpoint json");
        assert_eq!(
            diagnostic_json["config"]["frontier_policy"],
            LEGACY_DIAGNOSTIC_POLICY_NAME
        );
        assert_eq!(diagnostic_json["release_eligible"], false);
        assert!(!diagnostic.generate_report().release_eligible);
        let mut unmarked = diagnostic_json.clone();
        unmarked["config"]
            .as_object_mut()
            .expect("config object")
            .remove("frontier_policy");
        unmarked
            .as_object_mut()
            .expect("checkpoint object")
            .remove("release_eligible");
        assert_eq!(unmarked, json);
    }

    // r[verify chaoscontrol.campaign_policy.rollback.valid]
    #[test]
    fn campaign_resume_of_a_diagnostic_checkpoint_stays_release_ineligible() {
        let directory = tempfile::tempdir().expect("test directory");
        let output = directory.path().display().to_string();
        let diagnostic = Explorer::new(ExplorerConfig {
            frontier_policy: FrontierPolicy::LegacyDiagnostic,
            ..ExplorerConfig::default()
        });
        let mut checkpoint = diagnostic
            .create_checkpoint(&output)
            .expect("diagnostic checkpoint");
        checkpoint.config.frontier_policy = FrontierPolicy::Campaign;
        let resumed = Explorer::from_checkpoint(checkpoint, None, None, None).expect("resume");
        assert_eq!(resumed.config.frontier_policy, FrontierPolicy::Campaign);
        assert!(!resumed.release_eligible());
        let carried = resumed.create_checkpoint(&output).expect("checkpoint");
        assert!(!carried.release_eligible);
        assert!(!resumed.generate_report().release_eligible);

        let fresh = Explorer::new(ExplorerConfig::default())
            .create_checkpoint(&output)
            .expect("campaign checkpoint");
        let resumed = Explorer::from_checkpoint(fresh, None, None, None).expect("resume");
        assert!(resumed.release_eligible());
    }

    // r[verify chaoscontrol.system_runs.fault_injection.scenario.exploration]
    // r[verify chaoscontrol.system_runs.fault_injection.scenario.recorded]
    #[test]
    fn exploration_runs_carry_their_random_fault_setting_through_resume() {
        let directory = tempfile::tempdir().expect("test directory");
        let output = directory.path().display().to_string();
        assert!(ExplorerConfig::default().random_faults);
        for random_faults in [true, false] {
            let explorer = Explorer::new(ExplorerConfig {
                random_faults,
                ..ExplorerConfig::default()
            });
            assert_eq!(explorer.simulation_config().random_faults, random_faults);
            let checkpoint = explorer.create_checkpoint(&output).expect("checkpoint");
            assert_eq!(checkpoint.config.random_faults, random_faults);
            let resumed = Explorer::from_checkpoint(checkpoint, None, None, None).expect("resume");
            assert_eq!(resumed.simulation_config().random_faults, random_faults);
        }
    }

    const TEST_ALIAS: u32 = 42;
    const TEST_VARIANT_SEED: u64 = 73;
    const TEST_BRANCH_COUNT: usize = 2;
    const TEST_MARKER_TICK: u64 = 19;

    #[test]
    fn diversity_is_disabled_for_single_vcpu_and_explicitly_disabled_smp() {
        assert!(!schedule_diversity_enabled(true, 1));
        assert!(!schedule_diversity_enabled(false, MINIMUM_SMP_VCPUS));
        assert!(schedule_diversity_enabled(true, MINIMUM_SMP_VCPUS));
    }

    #[test]
    fn branch_work_preserves_generated_schedule_variants() {
        let variant = ScheduleVariant {
            scheduler_seed: TEST_VARIANT_SEED,
            strategy_override: None,
            quantum_override: None,
        };
        let variants = vec![
            (FaultSchedule::new(), Some(variant.clone())),
            (FaultSchedule::new(), None),
        ];
        let work = branch_work_from_variants(&variants);

        assert_eq!(work.len(), TEST_BRANCH_COUNT);
        assert_eq!(work[0].branch_index, 0);
        assert_eq!(work[0].schedule_variant, Some(variant));
        assert_eq!(work[1].branch_index, 1);
        assert!(work[1].schedule_variant.is_none());
    }

    fn failed_report(compatibility_id: Option<u32>) -> OracleReport {
        failed_report_for_process(compatibility_id, None)
    }

    fn failed_report_for_process(
        compatibility_id: Option<u32>,
        process: Option<&str>,
    ) -> OracleReport {
        let mut descriptor =
            crate::test_support::assertion_identity(u64::from(TEST_ALIAS)).descriptor;
        descriptor.compatibility_id = compatibility_id;
        let token = chaoscontrol_protocol::admission::token_for_descriptors(core::slice::from_ref(
            &descriptor,
        ))
        .expect("catalog token");
        let fingerprint = descriptor.fingerprint().expect("fingerprint");
        let mut builder =
            chaoscontrol_protocol::admission::CatalogBuilder::begin(1).expect("catalog begins");
        builder.insert(descriptor.clone()).expect("descriptor");
        let catalog = builder.complete(token).expect("catalog completes");
        let event = chaoscontrol_protocol::admission::BoundAssertionEvent {
            catalog_token: token,
            fingerprint,
            kind: descriptor.kind,
        };
        let mut oracle = chaoscontrol_fault::oracle::PropertyOracle::new();
        oracle.activate_catalog(catalog).expect("catalog activates");
        oracle.begin_run();
        let details = process.map(|process| {
            serde_json::to_vec(&serde_json::json!({
                "chaoscontrol_process_identity": process,
            }))
            .unwrap()
        });
        oracle
            .record_bound_event(&event, false, details.as_deref())
            .expect("failed event records");
        oracle.end_run();
        oracle.report()
    }

    fn branch_result(oracle_report: OracleReport) -> BranchResult {
        BranchResult {
            coverage: CoverageBitmap::new(),
            oracle_report,
            schedule: FaultSchedule::new(),
            exit_counts: Vec::new(),
            halted: false,
            total_ticks: TEST_MARKER_TICK,
            bugs: Vec::new(),
            snapshot: None,
            schedule_variant: None,
            schedule_fingerprint: 0,
            timings: BranchTimings::default(),
        }
    }

    #[test]
    fn process_scoped_failure_rejects_owner_loss_or_substitution_on_resume() {
        let report = failed_report_for_process(None, Some("writer"));
        let mut explorer = Explorer::new(ExplorerConfig::default());
        let bugs = explorer
            .extract_bugs(
                &branch_result(report.clone()),
                &FaultSchedule::new(),
                None,
                0,
                None,
            )
            .unwrap();
        let mut retained = SerializableBug::from(&bugs[0]);
        crate::checkpoint::validate_bug_set(std::slice::from_ref(&retained), Some(&report))
            .unwrap();
        for owners in [
            Default::default(),
            std::collections::BTreeSet::from(["checkpoint".to_string()]),
        ] {
            retained.process_instances = owners;
            assert!(crate::checkpoint::validate_bug_set(
                std::slice::from_ref(&retained),
                Some(&report)
            )
            .is_err());
        }
    }

    #[test]
    fn inherited_markers_do_not_create_new_branch_opportunities_or_exceed_run_bound() {
        use chaoscontrol_protocol::branch_marker::{
            BranchMarker, BRANCH_MARKER_EVENT, MAX_MARKERS_PER_RUN,
        };
        let mut oracle = chaoscontrol_fault::oracle::PropertyOracle::new();
        oracle.begin_run();
        for index in 0..MAX_MARKERS_PER_RUN {
            let marker = BranchMarker::new(
                "fixture",
                &format!("old-{index}"),
                "guest-0",
                serde_json::json!({}),
                None,
                None,
            )
            .unwrap();
            oracle
                .record_event(BRANCH_MARKER_EVENT, serde_json::to_value(marker).unwrap())
                .unwrap();
        }
        oracle.end_run();
        oracle.begin_run();
        oracle.end_run();
        let mut explorer = Explorer::new(ExplorerConfig::default());
        let result = branch_result(oracle.report());
        assert!(!explorer.should_retain_branch(&result, 0).unwrap());
        assert_eq!(
            crate::marker_branching::coverage_report(Vec::new(), &result.oracle_report)
                .unwrap()
                .reached
                .len(),
            MAX_MARKERS_PER_RUN
        );
        oracle.begin_run();
        let marker = BranchMarker::new(
            "fixture",
            "new",
            "guest-0",
            serde_json::json!({}),
            None,
            None,
        )
        .unwrap();
        let identity = marker.identity.clone();
        oracle
            .record_event(BRANCH_MARKER_EVENT, serde_json::to_value(marker).unwrap())
            .unwrap();
        oracle.end_run();
        let result = branch_result(oracle.report());
        let proposals = explorer.entry_proposals(&result, 1).unwrap();
        assert!(explorer.should_retain_branch(&result, 0).unwrap());
        assert_eq!(proposals.len(), 1);
        assert_eq!(
            proposals[0].marker.as_ref().unwrap().marker_identity,
            identity
        );
    }

    // r[verify chaoscontrol.event_branching.frontier_entry]
    #[test]
    fn repeated_markers_do_not_evict_a_distinct_deeper_snapshot() {
        use chaoscontrol_protocol::branch_marker::{BranchMarker, BRANCH_MARKER_EVENT};

        const OLD_EDGES: usize = 64;
        const CHILD_EDGES: usize = OLD_EDGES / 2;
        const REPEATED_OBSERVATIONS: usize = 16;
        for policy in [FrontierPolicy::Campaign, FrontierPolicy::LegacyDiagnostic] {
            let output = tempfile::tempdir().unwrap();
            let mut explorer = Explorer::new(ExplorerConfig {
                num_vms: 1,
                frontier_policy: policy,
                output_dir: Some(output.path().display().to_string()),
                max_frontier: 2,
                branch_factor: 1,
                max_rounds: 3,
                ..ExplorerConfig::default()
            });
            explorer.open_frontier(output.path()).unwrap();
            let snapshot = |tick| SimulationSnapshot {
                tick,
                vm_snapshots: Vec::new(),
                network_state: chaoscontrol_vmm::controller::NetworkFabric::new(
                    1,
                    TEST_VARIANT_SEED,
                ),
                fault_engine_snapshot: chaoscontrol_fault::engine::FaultEngine::new(
                    Default::default(),
                )
                .snapshot(),
                vcpu_stall_until: Vec::new(),
                clock_freeze: Vec::new(),
                clock_jitter_bound: Vec::new(),
                memory_pressure: Vec::new(),
                process_fault_attempt: Vec::new(),
                pending_process_observations: Default::default(),
                fault_operation_sequence: 0,
            };
            let mut root = branch_result(OracleReport::empty());
            root.snapshot = Some(snapshot(TEST_MARKER_TICK));
            explorer
                .admit_frontier_root(root, FaultSchedule::new())
                .unwrap();
            for (round, edges, key, repetitions) in [
                (0, OLD_EDGES, "old", REPEATED_OBSERVATIONS),
                (1, CHILD_EDGES, "child", 1),
            ] {
                let plan = match explorer
                    .frontier
                    .backend()
                    .unwrap()
                    .begin_round(
                        &mut explorer.rng,
                        RoundContext {
                            round,
                            mode: RoundMode::FaultSchedule,
                        },
                    )
                    .unwrap()
                {
                    RoundBegin::Planned(plan) => plan,
                    RoundBegin::Stop(stop) => panic!("unexpected stop {stop:?}"),
                };
                let authorized = match explorer
                    .frontier
                    .backend()
                    .unwrap()
                    .authorize(plan)
                    .unwrap()
                {
                    Authorization::Execute(round) => round,
                    Authorization::Skipped(_) => panic!("parent disappeared"),
                };
                assert_eq!(authorized.selected().unwrap().depth, round as u32);
                let mut oracle = chaoscontrol_fault::oracle::PropertyOracle::new();
                oracle.begin_run();
                for occurrence in 0..repetitions {
                    let marker = BranchMarker::new(
                        "fixture",
                        key,
                        "guest-0",
                        serde_json::json!({}),
                        None,
                        Some(format!("occurrence:{occurrence}")),
                    )
                    .unwrap();
                    oracle
                        .record_event(BRANCH_MARKER_EVENT, serde_json::to_value(marker).unwrap())
                        .unwrap();
                }
                oracle.end_run();
                let mut result = branch_result(oracle.report());
                result.total_ticks = TEST_MARKER_TICK + round + 1;
                result.snapshot = Some(snapshot(result.total_ticks));
                for edge in 0..edges {
                    result.coverage.record_hit(edge);
                }
                let schedule = FaultSchedule::new();
                let outcome = explorer
                    .branch_outcome(
                        BranchSlot::Variant(0),
                        edges,
                        &[],
                        Some((&result, &schedule, round as u32 + 1)),
                        true,
                    )
                    .unwrap();
                explorer
                    .frontier
                    .backend()
                    .unwrap()
                    .finish_round(
                        *authorized,
                        &RoundObservation {
                            branches: vec![outcome],
                        },
                    )
                    .unwrap();
            }
            let plan = match explorer
                .frontier
                .backend()
                .unwrap()
                .begin_round(
                    &mut explorer.rng,
                    RoundContext {
                        round: 2,
                        mode: RoundMode::FaultSchedule,
                    },
                )
                .unwrap()
            {
                RoundBegin::Planned(plan) => plan,
                RoundBegin::Stop(stop) => panic!("deeper parent unavailable: {stop:?}"),
            };
            let selected = match explorer
                .frontier
                .backend()
                .unwrap()
                .authorize(plan)
                .unwrap()
            {
                Authorization::Execute(round) => round,
                Authorization::Skipped(_) => panic!("deeper parent disappeared"),
            };
            let parent = selected.selected().unwrap();
            assert_eq!(
                parent.depth, 2,
                "repeated aliases displaced the distinct child under {policy:?}"
            );
            assert_eq!(parent.snapshot.tick, TEST_MARKER_TICK + 2);
            assert_eq!(
                parent.marker.as_ref().unwrap().marker_identity,
                chaoscontrol_protocol::branch_marker::marker_identity("fixture", "child")
            );
        }
    }

    // r[verify chaoscontrol.event_branching.frontier_entry]
    #[test]
    fn logical_marker_representatives_preserve_owners_and_best_context() {
        use chaoscontrol_protocol::branch_marker::{BranchMarker, BRANCH_MARKER_EVENT};

        let mut oracle = chaoscontrol_fault::oracle::PropertyOracle::new();
        oracle.begin_run();
        for (key, owner, position) in [
            ("repeated", "guest-0", "first"),
            ("repeated", "guest-0", "later"),
            ("repeated", "guest-1", "other-owner"),
            ("distinct", "guest-0", "other-key"),
        ] {
            let marker = BranchMarker::new(
                "fixture",
                key,
                owner,
                serde_json::json!({}),
                None,
                Some(position.to_string()),
            )
            .unwrap();
            oracle
                .record_event(BRANCH_MARKER_EVENT, serde_json::to_value(marker).unwrap())
                .unwrap();
        }
        oracle.end_run();
        let result = branch_result(oracle.report());
        let mut explorer = Explorer::new(ExplorerConfig::default());
        let proposals = explorer.entry_proposals(&result, 0).unwrap();
        let contexts = proposals
            .iter()
            .map(|proposal| {
                let marker = proposal.marker.as_ref().unwrap();
                crate::marker_branching::validate_parent_context(
                    marker,
                    &result.oracle_report,
                    TEST_MARKER_TICK,
                )
                .unwrap();
                (
                    marker.owner.as_str(),
                    marker.logical_position_ref.as_deref().unwrap(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            contexts,
            vec![
                ("guest-0", "first"),
                ("guest-1", "other-owner"),
                ("guest-0", "other-key")
            ]
        );
        assert!(proposals[0].score > proposals[1].score);
        let mut next = chaoscontrol_fault::oracle::PropertyOracle::new();
        next.begin_run();
        let marker = BranchMarker::new(
            "fixture",
            "repeated",
            "guest-0",
            serde_json::json!({}),
            None,
            None,
        )
        .unwrap();
        next.record_event(BRANCH_MARKER_EVENT, serde_json::to_value(marker).unwrap())
            .unwrap();
        next.end_run();
        let later = explorer
            .entry_proposals(&branch_result(next.report()), 0)
            .unwrap();
        assert!(
            later[0].score < proposals[1].score,
            "suppressed repeated observations must still contribute to rarity decay"
        );
    }

    // r[verify chaoscontrol.event_branching.validation]
    #[test]
    fn branch_markers_create_identity_bound_novel_frontier_entries() {
        let marker = chaoscontrol_protocol::branch_marker::BranchMarker::new(
            "raft",
            "leader-elected",
            "guest-0",
            serde_json::json!({"term": 1}),
            None,
            Some("term:1".to_string()),
        )
        .unwrap();
        let expected_identity = marker.identity.clone();
        let mut oracle = chaoscontrol_fault::oracle::PropertyOracle::new();
        oracle.begin_run();
        oracle
            .record_event(
                chaoscontrol_protocol::branch_marker::BRANCH_MARKER_EVENT,
                serde_json::to_value(marker).unwrap(),
            )
            .unwrap();
        oracle.end_run();

        let mut explorer = Explorer::new(ExplorerConfig::default());
        let proposals = explorer
            .entry_proposals(&branch_result(oracle.report()), 0)
            .unwrap();

        assert_eq!(proposals.len(), 1);
        let metadata = proposals[0].marker.as_ref().unwrap();
        assert_eq!(metadata.marker_identity, expected_identity);
        assert_eq!(metadata.observed_tick, TEST_MARKER_TICK);
        assert!(proposals[0].score >= crate::marker_branching::MARKER_NOVELTY_BONUS);
    }

    // r[verify chaoscontrol.event_branching.validation]
    #[test]
    fn marker_only_branches_remain_selectable_after_marker_budget_exhaustion() {
        use chaoscontrol_protocol::branch_marker::{
            BranchMarker, BRANCH_MARKER_EVENT, MAX_MARKERS_PER_RUN,
        };
        for (policy, num_vms) in [FrontierPolicy::Campaign, FrontierPolicy::LegacyDiagnostic]
            .into_iter()
            .flat_map(|policy| [1, 3].map(|num_vms| (policy, num_vms)))
        {
            let output = tempfile::tempdir().unwrap();
            let mut explorer = Explorer::new(ExplorerConfig {
                num_vms,
                frontier_policy: policy,
                output_dir: Some(output.path().display().to_string()),
                max_frontier: 1,
                branch_factor: 1,
                max_rounds: 2,
                ..ExplorerConfig::default()
            });
            explorer.open_frontier(output.path()).unwrap();
            let engine = chaoscontrol_fault::engine::FaultEngine::new(Default::default());
            let parent = SimulationSnapshot {
                tick: TEST_MARKER_TICK,
                vm_snapshots: Vec::new(),
                network_state: chaoscontrol_vmm::controller::NetworkFabric::new(
                    num_vms,
                    TEST_VARIANT_SEED,
                ),
                fault_engine_snapshot: engine.snapshot(),
                vcpu_stall_until: Vec::new(),
                clock_freeze: Vec::new(),
                clock_jitter_bound: Vec::new(),
                memory_pressure: Vec::new(),
                process_fault_attempt: Vec::new(),
                pending_process_observations: Default::default(),
                fault_operation_sequence: 0,
            };
            let mut root = branch_result(OracleReport::empty());
            root.snapshot = Some(parent.clone());
            explorer
                .admit_frontier_root(root, FaultSchedule::new())
                .unwrap();
            let descriptor =
                crate::test_support::assertion_identity(u64::from(TEST_ALIAS)).descriptor;
            let token = chaoscontrol_protocol::admission::token_for_descriptors(
                core::slice::from_ref(&descriptor),
            )
            .unwrap();
            let mut reports = Vec::with_capacity(num_vms);
            for vm_instance in 0..num_vms {
                let mut builder =
                    chaoscontrol_protocol::admission::CatalogBuilder::begin(1).unwrap();
                builder.insert(descriptor.clone()).unwrap();
                let mut oracle = chaoscontrol_fault::oracle::PropertyOracle::new();
                oracle
                    .activate_catalog(builder.complete(token).unwrap())
                    .unwrap();
                oracle.begin_run();
                for index in 0..=MAX_MARKERS_PER_RUN {
                    let marker = BranchMarker::new(
                        "fixture",
                        &format!("vm-{vm_instance}-rare-{index}"),
                        &format!("guest-{vm_instance}"),
                        serde_json::json!({}),
                        None,
                        None,
                    )
                    .unwrap();
                    let recorded = oracle
                        .record_event(BRANCH_MARKER_EVENT, serde_json::to_value(marker).unwrap());
                    if index == MAX_MARKERS_PER_RUN {
                        assert_eq!(
                            recorded,
                            Err(chaoscontrol_protocol::admission::CatalogConflict::MarkerLimitExceeded)
                        );
                    } else {
                        recorded.expect("marker within the admitted per-VM run budget");
                    }
                }
                oracle.end_run();
                reports.push((u32::try_from(vm_instance).unwrap(), oracle.report()));
            }
            let report = chaoscontrol_fault::report_merge::merge_oracle_reports(&reports).unwrap();
            let mut result = branch_result(report);
            result.snapshot = Some(parent);
            let coverage =
                crate::marker_branching::coverage_report(Vec::new(), &result.oracle_report)
                    .unwrap();
            assert_eq!(coverage.limit_events, num_vms);
            assert_eq!(coverage.reached.len(), MAX_MARKERS_PER_RUN * num_vms);
            assert!(explorer.should_retain_branch(&result, 0).unwrap());
            let plan = match explorer
                .frontier
                .backend()
                .unwrap()
                .begin_round(
                    &mut explorer.rng,
                    RoundContext {
                        round: 0,
                        mode: RoundMode::FaultSchedule,
                    },
                )
                .unwrap()
            {
                RoundBegin::Planned(plan) => plan,
                RoundBegin::Stop(stop) => panic!("unexpected stop {stop:?}"),
            };
            let authorized = match explorer
                .frontier
                .backend()
                .unwrap()
                .authorize(plan)
                .unwrap()
            {
                Authorization::Execute(round) => round,
                Authorization::Skipped(_) => panic!("parent disappeared"),
            };
            let schedule = FaultSchedule::new();
            let outcome = explorer
                .branch_outcome(
                    BranchSlot::Variant(0),
                    0,
                    &[],
                    Some((&result, &schedule, 1)),
                    true,
                )
                .unwrap();
            explorer
                .frontier
                .backend()
                .unwrap()
                .finish_round(
                    *authorized,
                    &RoundObservation {
                        branches: vec![outcome],
                    },
                )
                .unwrap();
            assert_eq!(explorer.frontier.len(), 1);
            let plan = match explorer
                .frontier
                .backend()
                .unwrap()
                .begin_round(
                    &mut explorer.rng,
                    RoundContext {
                        round: 1,
                        mode: RoundMode::FaultSchedule,
                    },
                )
                .unwrap()
            {
                RoundBegin::Planned(plan) => plan,
                RoundBegin::Stop(stop) => panic!("marker parent unavailable: {stop:?}"),
            };
            let selected = match explorer
                .frontier
                .backend()
                .unwrap()
                .authorize(plan)
                .unwrap()
            {
                Authorization::Execute(round) => round,
                Authorization::Skipped(_) => panic!("marker parent disappeared"),
            };
            let marker = selected
                .selected()
                .unwrap()
                .marker
                .as_ref()
                .expect("marker-only parent survives bounded pruning");
            marker.validate().unwrap();
            assert_eq!(marker.observed_tick, TEST_MARKER_TICK);
            assert!(!explorer
                .should_retain_branch(&branch_result(OracleReport::empty()), 0)
                .unwrap());
        }
    }

    #[test]
    fn bug_extraction_accepts_exact_identity_without_compatibility_alias() {
        let mut explorer = Explorer::new(ExplorerConfig::default());
        let result = branch_result(failed_report(None));
        let bugs = explorer
            .extract_bugs(&result, &FaultSchedule::new(), None, 0, None)
            .expect("exact failed assertion extracts");

        assert_eq!(bugs.len(), 1);
        assert_eq!(bugs[0].assertion_id, 0);
        assert_eq!(bugs[0].assertion_identity.descriptor.compatibility_id, None);
    }

    #[test]
    fn bug_dedup_key_is_deterministic_and_identity_bound() {
        let first = crate::test_support::assertion_identity(u64::from(TEST_ALIAS));
        let second = crate::test_support::assertion_identity(u64::from(TEST_ALIAS + 1));
        let schedule = FaultSchedule::new();
        let key =
            Explorer::compute_dedup_key(first.fingerprint, &schedule).expect("first dedup key");

        assert_eq!(
            Explorer::compute_dedup_key(first.fingerprint, &schedule).expect("repeat dedup key"),
            key
        );
        assert_ne!(
            Explorer::compute_dedup_key(second.fingerprint, &schedule).expect("second dedup key"),
            key
        );
    }

    #[test]
    fn bug_extraction_rejects_invalid_failed_report() {
        let mut report = failed_report(Some(TEST_ALIAS));
        let record = report
            .structured_assertions
            .values_mut()
            .next()
            .expect("failed record");
        record.identity = None;
        let mut explorer = Explorer::new(ExplorerConfig::default());
        let result = branch_result(report);

        assert!(explorer
            .extract_bugs(&result, &FaultSchedule::new(), None, 0, None)
            .is_err());
        assert!(explorer.seen_dedup_keys.is_empty());
    }

    #[test]
    fn nickel_assertion_fixture_round_trips_through_rust_type() {
        let json = include_str!("../../../contracts/evidence/fixtures/valid/assertions.valid.json");
        let details: Vec<AssertionDetail> = serde_json::from_str(json).unwrap();
        assert_eq!(details.len(), 42);
        assert!(details
            .iter()
            .all(|detail| matches!(detail.verdict.as_str(), "passed" | "failed" | "unexercised")));

        let roundtrip = serde_json::to_string(&details).unwrap();
        let reparsed: Vec<AssertionDetail> = serde_json::from_str(&roundtrip).unwrap();
        assert_eq!(details.len(), reparsed.len());
        assert_eq!(details[0].id, reparsed[0].id);
    }

    #[test]
    fn test_explorer_config_default() {
        let config = ExplorerConfig::default();
        assert_eq!(config.num_vms, 2);
        assert_eq!(config.seed, 42);
        assert_eq!(config.branch_factor, 8);
    }

    #[test]
    fn test_explorer_new() {
        let config = ExplorerConfig {
            kernel_path: "/nonexistent".to_string(),
            ..Default::default()
        };
        let explorer = Explorer::new(config);
        assert_eq!(explorer.rounds_completed, 0);
        assert_eq!(explorer.total_branches_run, 0);
    }

    #[test]
    fn test_explorer_stats_initial() {
        let config = ExplorerConfig {
            kernel_path: "/nonexistent".to_string(),
            ..Default::default()
        };
        let explorer = Explorer::new(config);
        let stats = explorer.stats();

        assert_eq!(stats.rounds, 0);
        assert_eq!(stats.branches, 0);
        assert_eq!(stats.edges, 0);
        assert_eq!(stats.bugs, 0);
    }

    #[test]
    fn test_explorer_score_branch() {
        let config = ExplorerConfig::default();
        let explorer = Explorer::new(config);

        let mut result = BranchResult {
            coverage: CoverageBitmap::new(),
            oracle_report: OracleReport {
                total_runs: 1,
                ..OracleReport::empty()
            },
            schedule: FaultSchedule::new(),
            exit_counts: vec![100],
            halted: false,
            total_ticks: 100,
            bugs: Vec::new(),
            snapshot: None,
            schedule_variant: None,
            schedule_fingerprint: 0,
            timings: BranchTimings::default(),
        };

        // Add some coverage
        for i in 0..10 {
            result.coverage.record_hit(i);
        }

        let score = explorer.score_branch(&result, 0);
        assert!(score > 0.0);
    }

    #[test]
    fn test_explorer_assertion_coverage_blind_mode() {
        let config = ExplorerConfig::default();
        let explorer = Explorer::new(config);

        let mut oracle = OracleReport {
            total_runs: 1,
            ..OracleReport::empty()
        };

        oracle.assertions.insert(
            10,
            chaoscontrol_fault::oracle::AssertionRecord {
                message: "test".to_string(),
                kind: chaoscontrol_fault::oracle::AssertionKind::Always,
                hit_count: 1,
                true_count: 1,
                false_count: 0,
                runs_hit: 1,
                runs_satisfied: 1,
                first_failure_run: None,
                last_failure_details: None,
                guest: "uncategorized".to_string(),
                category: "uncategorized".to_string(),
                identity: None,
                compatibility_id: Some(10),
                catalog_tokens: std::collections::BTreeSet::new(),
                vm_instances: std::collections::BTreeSet::new(),
                process_instances: std::collections::BTreeSet::new(),
                fallback_scope: None,
            },
        );

        let coverage = explorer.assertion_coverage(&oracle);
        assert!(coverage.count_bits() > 0);
    }

    // ── Protocol event enrichment tests ──────────────────────────

    fn make_oracle_report() -> OracleReport {
        OracleReport {
            total_runs: 1,
            ..OracleReport::empty()
        }
    }

    #[test]
    fn test_event_enrichment_different_events_different_coverage() {
        let mut report_a = make_oracle_report();
        report_a
            .events
            .push(chaoscontrol_fault::oracle::OracleEvent {
                run_id: 0,
                name: "commit".to_string(),
                details: serde_json::json!({"index": 1}),
            });

        let mut report_b = make_oracle_report();
        report_b
            .events
            .push(chaoscontrol_fault::oracle::OracleEvent {
                run_id: 0,
                name: "commit".to_string(),
                details: serde_json::json!({"index": 5}),
            });

        let mut cov_a = CoverageBitmap::new();
        let mut cov_b = CoverageBitmap::new();
        Explorer::enrich_with_protocol_events(&mut cov_a, &report_a);
        Explorer::enrich_with_protocol_events(&mut cov_b, &report_b);

        // Both should have hits, but differ
        assert!(cov_a.count_bits() > 0);
        assert!(cov_b.count_bits() > 0);
        assert_ne!(cov_a.as_slice(), cov_b.as_slice());
    }

    #[test]
    fn test_event_enrichment_no_events_no_enrichment() {
        let report = make_oracle_report();
        let mut coverage = CoverageBitmap::new();
        Explorer::enrich_with_protocol_events(&mut coverage, &report);
        assert_eq!(coverage.count_bits(), 0);
    }

    #[test]
    fn test_event_enrichment_event_name_hashed() {
        let mut report_a = make_oracle_report();
        report_a
            .events
            .push(chaoscontrol_fault::oracle::OracleEvent {
                run_id: 0,
                name: "leader_elected".to_string(),
                details: serde_json::json!({}),
            });

        let mut report_b = make_oracle_report();
        report_b
            .events
            .push(chaoscontrol_fault::oracle::OracleEvent {
                run_id: 0,
                name: "follower_timeout".to_string(),
                details: serde_json::json!({}),
            });

        let mut cov_a = CoverageBitmap::new();
        let mut cov_b = CoverageBitmap::new();
        Explorer::enrich_with_protocol_events(&mut cov_a, &report_a);
        Explorer::enrich_with_protocol_events(&mut cov_b, &report_b);

        assert!(cov_a.count_bits() > 0);
        assert!(cov_b.count_bits() > 0);
        assert_ne!(cov_a.as_slice(), cov_b.as_slice());
    }

    #[test]
    fn test_event_enrichment_slots_in_assertion_region() {
        let mut report = make_oracle_report();
        report.events.push(chaoscontrol_fault::oracle::OracleEvent {
            run_id: 0,
            name: "commit".to_string(),
            details: serde_json::json!({"index": 42}),
        });

        let mut coverage = CoverageBitmap::new();
        Explorer::enrich_with_protocol_events(&mut coverage, &report);

        // All hits should be in assertion region [CODE_REGION_END, ASSERTION_REGION_END)
        let slice = coverage.as_slice();
        for (i, &val) in slice[..crate::coverage::CODE_REGION_END].iter().enumerate() {
            assert_eq!(val, 0, "code region slot {} should be 0", i);
        }
        for (i, &val) in slice[crate::coverage::ASSERTION_REGION_END..]
            .iter()
            .enumerate()
        {
            assert_eq!(
                val,
                0,
                "schedule region slot {} should be 0",
                crate::coverage::ASSERTION_REGION_END + i
            );
        }
        assert!(coverage.count_bits() > 0);
    }

    // ── Assertion detail enrichment tests ────────────────────────

    #[test]
    fn test_assertion_detail_different_values_different_coverage() {
        let mut report_a = make_oracle_report();
        report_a.assertions.insert(
            1,
            chaoscontrol_fault::oracle::AssertionRecord {
                message: "election safety".to_string(),
                kind: chaoscontrol_fault::oracle::AssertionKind::Always,
                hit_count: 1,
                true_count: 0,
                false_count: 1,
                runs_hit: 1,
                runs_satisfied: 0,
                first_failure_run: Some(0),
                last_failure_details: Some(b"{\"term\":3}".to_vec()),
                guest: "uncategorized".to_string(),
                category: "uncategorized".to_string(),
                identity: None,
                compatibility_id: Some(1),
                catalog_tokens: std::collections::BTreeSet::new(),
                vm_instances: std::collections::BTreeSet::new(),
                process_instances: std::collections::BTreeSet::new(),
                fallback_scope: None,
            },
        );

        let mut report_b = make_oracle_report();
        report_b.assertions.insert(
            1,
            chaoscontrol_fault::oracle::AssertionRecord {
                message: "election safety".to_string(),
                kind: chaoscontrol_fault::oracle::AssertionKind::Always,
                hit_count: 1,
                true_count: 0,
                false_count: 1,
                runs_hit: 1,
                runs_satisfied: 0,
                first_failure_run: Some(0),
                last_failure_details: Some(b"{\"term\":5}".to_vec()),
                guest: "uncategorized".to_string(),
                category: "uncategorized".to_string(),
                identity: None,
                compatibility_id: Some(1),
                catalog_tokens: std::collections::BTreeSet::new(),
                vm_instances: std::collections::BTreeSet::new(),
                process_instances: std::collections::BTreeSet::new(),
                fallback_scope: None,
            },
        );

        let mut cov_a = CoverageBitmap::new();
        let mut cov_b = CoverageBitmap::new();
        Explorer::enrich_with_assertion_state(&mut cov_a, &report_a);
        Explorer::enrich_with_assertion_state(&mut cov_b, &report_b);

        assert!(cov_a.count_bits() > 0);
        assert!(cov_b.count_bits() > 0);
        assert_ne!(cov_a.as_slice(), cov_b.as_slice());
    }

    #[test]
    fn test_assertion_detail_null_details_no_extra_hashing() {
        let mut report = make_oracle_report();
        report.assertions.insert(
            1,
            chaoscontrol_fault::oracle::AssertionRecord {
                message: "test".to_string(),
                kind: chaoscontrol_fault::oracle::AssertionKind::Always,
                hit_count: 1,
                true_count: 1,
                false_count: 0,
                runs_hit: 1,
                runs_satisfied: 1,
                first_failure_run: None,
                last_failure_details: None,
                guest: "uncategorized".to_string(),
                category: "uncategorized".to_string(),
                identity: None,
                compatibility_id: Some(1),
                catalog_tokens: std::collections::BTreeSet::new(),
                vm_instances: std::collections::BTreeSet::new(),
                process_instances: std::collections::BTreeSet::new(),
                fallback_scope: None,
            },
        );

        let mut cov_with = CoverageBitmap::new();
        Explorer::enrich_with_assertion_state(&mut cov_with, &report);

        // Should still have enrichment from verdict/hit-count, but no detail hashing
        // The count should be the same as without any detail hashing
        let bits = cov_with.count_bits();
        assert!(bits > 0); // verdict + ratio hashing
        assert!(bits <= 3); // at most: verdict hash + ratio hash (no detail hashes)
    }

    // Integration tests with real VMs would go here, marked #[ignore]
    // They require a kernel and would be run separately.

    #[test]
    #[ignore]
    fn test_explorer_run_integration() {
        let config = ExplorerConfig {
            kernel_path: "/path/to/vmlinux".to_string(),
            initrd_path: Some("/path/to/initrd".to_string()),
            num_vms: 2,
            branch_factor: 4,
            max_rounds: 2,
            ticks_per_branch: 100,
            ..Default::default()
        };

        let mut explorer = Explorer::new(config);
        let report = explorer.run().unwrap();

        assert!(report.rounds <= 2);
        assert!(report.total_branches > 0);
    }

    #[test]
    fn test_explorer_checkpoint_roundtrip() {
        use std::fs;

        let tempdir = std::env::temp_dir();
        let checkpoint_dir = tempdir.join("test_checkpoint_roundtrip");
        let _ = fs::create_dir_all(&checkpoint_dir);

        let config = ExplorerConfig {
            kernel_path: "/fake/kernel".to_string(),
            initrd_path: Some("/fake/initrd".to_string()),
            num_vms: 3,
            seed: 12345,
            branch_factor: 16,
            ticks_per_branch: 2000,
            max_rounds: 200,
            max_frontier: 100,
            quantum: 200,
            output_dir: Some(checkpoint_dir.to_string_lossy().to_string()),
            ..Default::default()
        };

        let mut explorer = Explorer::new(config);

        // Simulate some progress
        explorer.rounds_completed = 42;
        explorer.total_branches_run = 336;

        // Add some coverage
        let mut bitmap = CoverageBitmap::new();
        for i in 0..100 {
            bitmap.record_hit(i * 10);
        }
        explorer.coverage.update_global(&bitmap);

        // Save checkpoint
        explorer
            .save_checkpoint_to_dir(&checkpoint_dir.to_string_lossy())
            .unwrap();

        // Load checkpoint
        let checkpoint_path = checkpoint_dir.join("checkpoint.json");
        let checkpoint = crate::checkpoint::load_checkpoint(&checkpoint_path).unwrap();

        // Verify checkpoint contents
        assert_eq!(checkpoint.config.num_vms, 3);
        assert_eq!(checkpoint.config.seed, 12345);
        assert_eq!(checkpoint.rounds_completed, 42);
        assert_eq!(checkpoint.total_branches_run, 336);
        assert_eq!(checkpoint.total_edges, 100);

        // Create new explorer from checkpoint
        let restored = Explorer::from_checkpoint(
            checkpoint,
            Some("/fake/kernel".to_string()),
            Some("/fake/initrd".to_string()),
            Some(200),
        )
        .expect("valid checkpoint restores");

        assert_eq!(restored.rounds_completed, 42);
        assert_eq!(restored.total_branches_run, 336);
        assert_eq!(restored.config.num_vms, 3);
        assert_eq!(restored.config.seed, 12345);
        assert_eq!(restored.coverage.stats().total_edges, 100);

        // Cleanup
        let _ = fs::remove_dir_all(&checkpoint_dir);
    }
}
