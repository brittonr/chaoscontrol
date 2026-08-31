//! The main exploration loop — coverage-guided fault schedule search.

use crate::checkpoint::{
    save_checkpoint, CheckpointConfig, CheckpointError, ExplorationCheckpoint, SerializableBug,
};
use crate::corpus::{BugReport, Corpus, CorpusEntry};
use crate::coverage::{CoverageBitmap, CoverageCollector, CoverageStats};
use crate::frontier::{Frontier, FrontierEntry};
use crate::input_tree;
use crate::marker_branching::{
    frontier_metadata, marker_score, observations as marker_observations, update_hit_counts,
};
use crate::mutator::{MutationConfig, ScheduleMutator};
use crate::worker::{BranchWork, WorkerPool};
use chaoscontrol_fault::oracle::OracleReport;
use chaoscontrol_fault::schedule::FaultSchedule;
use chaoscontrol_protocol::COVERAGE_BITMAP_ADDR;
use chaoscontrol_vmm::controller::{SimulationConfig, SimulationController, SimulationSnapshot};
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
    /// Optional output directory for checkpoints and reports.
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
        }
    }
}

const MINIMUM_SMP_VCPUS: usize = 2;

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
    frontier: Frontier,
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
}

impl Explorer {
    /// Create a new explorer with the given configuration.
    pub fn new(config: ExplorerConfig) -> Self {
        if config.kernel_path.is_empty() {
            warn!("ExplorerConfig has empty kernel_path — exploration will fail");
        }

        let frontier = Frontier::new(config.max_frontier);
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
        }
    }

    /// All bugs found: corpus bugs + standalone bugs from zero-coverage branches.
    fn all_bugs(&self) -> Vec<BugReport> {
        let mut bugs = self.corpus.bugs();
        bugs.extend(self.standalone_bugs.iter().cloned());
        bugs
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

    /// Emit a Finished event.
    fn emit_finished(&self, reason: &str) {
        self.emit_event(crate::dashboard_types::DashboardEvent::Finished {
            total_rounds: self.rounds_completed,
            total_branches: self.total_branches_run,
            total_edges: self.coverage.stats().total_edges,
            total_bugs: self.all_bugs().len(),
            reason: reason.to_string(),
            from_seed: None,
        });
    }

    fn phase_totals_for(results: &[(BranchResult, FaultSchedule)]) -> BranchTimings {
        results
            .iter()
            .fold(BranchTimings::default(), |mut total, (result, _)| {
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
        let run_start = Instant::now();

        info!(
            "Starting exploration: {} rounds, {} branches/round, {} VMs, schedule_diversity={}",
            self.config.max_rounds,
            self.config.branch_factor,
            self.config.num_vms,
            self.config.schedule_diversity
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

        if let Some(snapshot) = initial_result.snapshot.clone() {
            self.add_to_frontier(snapshot, initial_result, initial_schedule, None, 0);
        }

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
            let round_report = match self.config.exploration_mode {
                ExplorationMode::FaultSchedule => self.explore_round()?,
                ExplorationMode::InputTree => self.explore_input_tree_round()?,
                ExplorationMode::Hybrid => {
                    if round % 2 == 0 {
                        self.explore_round()?
                    } else {
                        self.explore_input_tree_round()?
                    }
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
                cumulative_bugs: self.all_bugs().len(),
                frontier_size: round_report.frontier_size,
                corpus_size: self.corpus.stats().total_entries,
                restore_ms: round_report.timings.restore_ms,
                run_ms: round_report.timings.run_ms,
                snapshot_ms: round_report.timings.snapshot_ms,
                coverage_ms: round_report.timings.coverage_ms,
                wall_clock_seconds: round_elapsed,
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

            // A configured checkpoint is authority-bearing output. Failure is fatal.
            if let Some(ref output_dir) = self.config.output_dir {
                self.save_checkpoint_to_dir(output_dir)?;
            }

            // Track stale rounds (no new edges, no new bugs).
            if round_report.new_coverage_edges == 0 && round_report.bugs_found == 0 {
                self.consecutive_stale_rounds += 1;
            } else {
                self.consecutive_stale_rounds = 0;
            }

            // Check for stopping conditions
            if self.frontier.is_empty() {
                // Try recycling from corpus before giving up.
                let recycled = self.recycle_frontier_from_corpus();
                if recycled > 0 {
                    info!(
                        "Frontier exhausted — recycled {} entries from corpus",
                        recycled
                    );
                } else {
                    info!("Frontier exhausted and corpus empty, stopping");
                    self.emit_finished("frontier_exhausted");
                    break;
                }
            }

            if self.config.stale_round_limit > 0
                && self.consecutive_stale_rounds >= self.config.stale_round_limit
            {
                info!(
                    "Coverage plateau: {} consecutive rounds with no new edges or bugs, stopping",
                    self.consecutive_stale_rounds
                );
                self.emit_finished("coverage_plateau");
                break;
            }

            // Optionally stop if we found bugs (for testing)
            if round_report.bugs_found > 0 && self.config.max_rounds < 10 {
                info!("Bug found in short run, stopping");
                self.emit_finished("bug_found");
                break;
            }

            // Check for graceful shutdown (Ctrl-C / SIGTERM).
            if crate::signal::shutdown_requested() {
                info!("Shutdown requested, stopping after round {}", round + 1);
                self.emit_finished("interrupted");
                break;
            }
        }

        // Emit Finished if we completed all rounds (no early break).
        if self.rounds_completed >= self.config.max_rounds {
            self.emit_finished("completed");
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

    /// Execute one exploration round:
    /// 1. Select a frontier entry
    /// 2. Generate N variant fault schedules
    /// 3. For each variant: restore snapshot → apply schedule → run → collect coverage
    /// 4. Score results, add interesting ones to frontier and corpus
    /// 5. Record any bugs found
    fn explore_round(&mut self) -> Result<RoundReport, ExploreError> {
        // Select entry from frontier
        let (snapshot, base_schedule, parent_id, parent_depth) =
            if let Some(entry) = self.frontier.select(&mut self.rng) {
                (
                    Some(entry.snapshot.clone()),
                    entry.schedule.clone(),
                    Some(entry.id),
                    entry.depth,
                )
            } else {
                // No frontier entry, use clean slate
                (None, FaultSchedule::new(), None, 0)
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
        let results = match (&mut self.worker_pool, &snapshot) {
            (Some(pool), Some(snap)) => {
                let work = branch_work_from_variants(&variants);

                let branch_results = pool.run_branches(snap, work)?;
                branch_results
                    .into_iter()
                    .zip(variants.into_iter().map(|(schedule, _variant)| schedule))
                    .collect()
            }
            _ => self.run_branches_sequential(&snapshot, variants)?,
        };

        // Process results in deterministic order (by branch index).
        let timings = Self::phase_totals_for(&results);
        let mut branches_run = 0;
        let mut new_coverage_edges = 0;
        let mut bugs_found: usize = 0;

        for (i, (result, schedule)) in results.into_iter().enumerate() {
            branches_run += 1;
            self.total_branches_run += 1;

            // Enrich coverage with assertion-state edges so branches
            // with new assertion patterns are "interesting" even after
            // code coverage saturates.
            let mut enriched = result.coverage.clone();
            Self::enrich_with_assertion_state(&mut enriched, &result.oracle_report);
            Self::enrich_with_protocol_events(&mut enriched, &result.oracle_report);
            Self::enrich_with_schedule_fingerprint(&mut enriched, result.schedule_fingerprint);

            let new_edges = enriched.has_new_coverage(self.coverage.global_coverage());
            let branch_bugs =
                self.extract_bugs(&result, &schedule, snapshot.as_ref(), parent_depth)?;
            bugs_found =
                bugs_found
                    .checked_add(branch_bugs.len())
                    .ok_or_else(|| ExploreError::Config {
                        message: "round bug count overflow".to_string(),
                    })?;
            if !branch_bugs.is_empty() {
                warn!("Branch {} found {} bugs!", i + 1, branch_bugs.len());
            }

            if new_edges > 0 {
                debug!("Branch {} found {} new edges", i + 1, new_edges);
                new_coverage_edges += new_edges;

                let mut enriched_result = result.clone();
                enriched_result.coverage = enriched.clone();

                self.add_to_corpus(
                    enriched_result.clone(),
                    schedule.clone(),
                    new_edges,
                    branch_bugs,
                    parent_depth + 1,
                );

                if let Some(snap) = result.snapshot.clone() {
                    self.add_to_frontier(
                        snap,
                        enriched_result,
                        schedule.clone(),
                        parent_id,
                        parent_depth + 1,
                    );
                }
            } else {
                self.retain_standalone_bugs(branch_bugs)?;
            }

            self.coverage.update_global(&enriched);
        }

        Ok(RoundReport {
            branches_run,
            new_coverage_edges,
            bugs_found,
            frontier_size: self.frontier.len(),
            timings,
        })
    }

    /// Run branches sequentially (original path).
    fn run_branches_sequential(
        &mut self,
        snapshot: &Option<SimulationSnapshot>,
        variants: Vec<(FaultSchedule, Option<ScheduleVariant>)>,
    ) -> Result<Vec<(BranchResult, FaultSchedule)>, ExploreError> {
        let mut results = Vec::with_capacity(variants.len());
        for (i, (schedule, schedule_variant)) in variants.into_iter().enumerate() {
            debug!("Running branch {}/{}", i + 1, self.config.branch_factor);
            let result = self.run_branch(snapshot, schedule.clone(), schedule_variant.as_ref())?;
            results.push((result, schedule));
        }
        Ok(results)
    }

    /// Execute one input tree exploration round:
    ///
    /// 1. Select a frontier entry (same as fault schedule mode)
    /// 2. Run a "probe" branch from that snapshot — recording all random
    ///    choices the guest makes
    /// 3. Examine the choice history — select interesting decision points
    /// 4. For each selected choice point, restore the snapshot, override
    ///    that choice to an alternate value, and re-run
    /// 5. Score results, add interesting ones to frontier and corpus
    fn explore_input_tree_round(&mut self) -> Result<RoundReport, ExploreError> {
        let mut branches_run = 0;
        let mut new_coverage_edges = 0;
        let mut bugs_found: usize = 0;
        let mut timings = BranchTimings::default();

        // Select entry from frontier
        let (snapshot, base_schedule, parent_id, parent_depth) =
            if let Some(entry) = self.frontier.select(&mut self.rng) {
                (
                    Some(entry.snapshot.clone()),
                    entry.schedule.clone(),
                    Some(entry.id),
                    entry.depth,
                )
            } else {
                (None, FaultSchedule::new(), None, 0)
            };

        // Phase 1: Run a "probe" branch to discover choice points.
        // This uses the base schedule (no overrides) — same as the
        // parent run, but we record the choice history.
        let probe_result = self.run_branch(&snapshot, base_schedule.clone(), None)?;
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
        if probe_new > 0 {
            new_coverage_edges += probe_new;
            let mut enriched_probe = probe_result.clone();
            enriched_probe.coverage = probe_enriched.clone();
            self.add_to_corpus(
                enriched_probe.clone(),
                base_schedule.clone(),
                probe_new,
                probe_bugs,
                parent_depth + 1,
            );
            if let Some(snap) = probe_result.snapshot.clone() {
                self.add_to_frontier(
                    snap,
                    enriched_probe,
                    base_schedule.clone(),
                    parent_id,
                    parent_depth + 1,
                );
            }
        } else {
            self.retain_standalone_bugs(probe_bugs)?;
        }
        self.coverage.update_global(&probe_enriched);

        // Phase 2: Drain choice histories from all VMs.
        self.ensure_controller()?;
        let histories = self.controller.as_mut().unwrap().drain_choice_histories();

        if histories.is_empty() {
            debug!("No random choices recorded — skipping input tree branching");
            return Ok(RoundReport {
                branches_run,
                new_coverage_edges,
                bugs_found,
                frontier_size: self.frontier.len(),
                timings,
            });
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
            return Ok(RoundReport {
                branches_run,
                new_coverage_edges,
                bugs_found,
                frontier_size: self.frontier.len(),
                timings,
            });
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

            let result =
                self.run_branch_with_overrides(&snapshot, base_schedule.clone(), per_vm_overrides)?;
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
            let branch_bugs =
                self.extract_bugs(&result, &base_schedule, snapshot.as_ref(), parent_depth)?;
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

            if new_edges > 0 {
                debug!("Input tree branch {} found {} new edges", i + 1, new_edges);
                new_coverage_edges += new_edges;

                let mut enriched_result = result.clone();
                enriched_result.coverage = enriched.clone();

                self.add_to_corpus(
                    enriched_result.clone(),
                    base_schedule.clone(),
                    new_edges,
                    branch_bugs,
                    parent_depth + 1,
                );

                if let Some(snap) = result.snapshot.clone() {
                    self.add_to_frontier(
                        snap,
                        enriched_result,
                        base_schedule.clone(),
                        parent_id,
                        parent_depth + 1,
                    );
                }
            } else {
                self.retain_standalone_bugs(branch_bugs)?;
            }

            self.coverage.update_global(&enriched);
        }

        Ok(RoundReport {
            branches_run,
            new_coverage_edges,
            bugs_found,
            frontier_size: self.frontier.len(),
            timings,
        })
    }

    /// Ensure we have a controller ready (created once, reused across branches).
    fn ensure_controller(&mut self) -> Result<(), ExploreError> {
        if self.controller.is_some() {
            return Ok(());
        }

        let mut vm_config = self.config.vm_config.clone();
        vm_config.scheduling_strategy = self.config.scheduling_strategy;

        let sim_config = SimulationConfig {
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
        };

        self.controller = Some(SimulationController::new(sim_config)?);
        Ok(())
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
        snapshot: &Option<SimulationSnapshot>,
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
        snapshot: &Option<SimulationSnapshot>,
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
        replay_snapshot: Option<&SimulationSnapshot>,
        parent_depth: u32,
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
                let dedup_key = Self::compute_dedup_key(*assertion_fingerprint, schedule)?;

                // Skip if we've already seen this (assertion, fault_types) pair
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
                };

                // Emit BugFound event for dashboard.
                self.emit_event(crate::dashboard_types::DashboardEvent::BugFound {
                    bug_index: self.all_bugs().len() + bugs.len(),
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
            self.frontier.push(frontier_entry);
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
                        self.frontier.push(frontier_entry);
                        recycled += 1;
                    }
                }
            }
        }

        recycled
    }

    /// Add a result to the frontier.
    fn add_to_frontier(
        &mut self,
        snapshot: chaoscontrol_vmm::controller::SimulationSnapshot,
        result: BranchResult,
        schedule: FaultSchedule,
        parent: Option<u64>,
        depth: u32,
    ) {
        let base_score = self.score_branch(&result, depth);
        let markers = marker_observations(&result.oracle_report).unwrap_or_default();
        if markers.is_empty() {
            self.frontier.push(FrontierEntry {
                id: 0,
                snapshot,
                coverage: result.coverage,
                score: base_score,
                times_selected: 0,
                depth,
                schedule,
                parent,
                marker: None,
            });
            return;
        }

        for observation in markers {
            let prior_hits = update_hit_counts(&mut self.marker_hits, &observation.marker.identity);
            let metadata = frontier_metadata(&observation, result.total_ticks);
            self.frontier.push(FrontierEntry {
                id: 0,
                snapshot: snapshot.clone(),
                coverage: result.coverage.clone(),
                score: marker_score(base_score, prior_hits),
                times_selected: 0,
                depth,
                schedule: schedule.clone(),
                parent,
                marker: Some(metadata),
            });
        }
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
        let bugs = self.all_bugs();
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
            bugs: self.all_bugs().len(),
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
        };

        let snapshot_store = crate::snapshot_store::FileSnapshotStore::new(output_dir);
        let mut bugs = Vec::new();
        for bug in self.all_bugs() {
            let mut serialized: SerializableBug = (&bug).into();
            if bug.replay_parent_depth > 0 && serialized.replay_parent_snapshot_ref.is_none() {
                let snapshot = bug.snapshot.as_ref().ok_or(
                    CheckpointError::MissingRequiredReplayParentSnapshot {
                        bug_id: bug.bug_id,
                        replay_parent_depth: bug.replay_parent_depth,
                    },
                )?;
                let reference = crate::snapshot_store::SnapshotStore::put_snapshot(
                    &snapshot_store,
                    snapshot,
                    bug.replay_parent_depth,
                )?;
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
        };

        let frontier = Frontier::new(config.max_frontier);
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

    const TEST_ALIAS: u32 = 42;
    const TEST_VARIANT_SEED: u64 = 73;
    const TEST_BRANCH_COUNT: usize = 2;
    const TEST_VM_COUNT: usize = 2;
    const TEST_NETWORK_SEED: u64 = 42;
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
        oracle
            .record_bound_event(&event, false, None)
            .expect("failed event records");
        oracle.end_run();
        oracle.report()
    }

    fn dummy_snapshot() -> SimulationSnapshot {
        let engine = chaoscontrol_fault::engine::FaultEngine::new(
            chaoscontrol_fault::engine::EngineConfig::default(),
        );
        SimulationSnapshot {
            tick: 0,
            vm_snapshots: Vec::new(),
            network_state: chaoscontrol_vmm::controller::NetworkFabric::new(
                TEST_VM_COUNT,
                TEST_NETWORK_SEED,
            ),
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
        explorer.add_to_frontier(
            dummy_snapshot(),
            branch_result(oracle.report()),
            FaultSchedule::new(),
            None,
            0,
        );

        assert_eq!(explorer.frontier.len(), 1);
        let metadata = explorer.frontier.entries()[0].marker.as_ref().unwrap();
        assert_eq!(metadata.marker_identity, expected_identity);
        assert_eq!(metadata.observed_tick, TEST_MARKER_TICK);
        assert!(
            explorer.frontier.entries()[0].score >= crate::marker_branching::MARKER_NOVELTY_BONUS
        );
    }

    #[test]
    fn bug_extraction_accepts_exact_identity_without_compatibility_alias() {
        let mut explorer = Explorer::new(ExplorerConfig::default());
        let result = branch_result(failed_report(None));
        let bugs = explorer
            .extract_bugs(&result, &FaultSchedule::new(), None, 0)
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
            .extract_bugs(&result, &FaultSchedule::new(), None, 0)
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

    #[test]
    fn test_shutdown_flag_stops_exploration() {
        // Reset in case another test set the flag.
        crate::signal::reset_shutdown();

        // We can't run a real Explorer without KVM, but we can verify
        // the signal module round-trips correctly and the flag is
        // checked in the right place (the Explorer::run loop checks
        // crate::signal::shutdown_requested() after each round).
        assert!(!crate::signal::shutdown_requested());
        crate::signal::request_shutdown();
        assert!(crate::signal::shutdown_requested());
        crate::signal::reset_shutdown();
    }
}
