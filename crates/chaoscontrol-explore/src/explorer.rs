//! The main exploration loop — coverage-guided fault schedule search.

use crate::checkpoint::{
    save_checkpoint, CheckpointConfig, CheckpointError, ExplorationCheckpoint, SerializableBug,
};
use crate::corpus::{BugReport, Corpus, CorpusEntry};
use crate::coverage::{CoverageBitmap, CoverageCollector, CoverageStats};
use crate::frontier::{Frontier, FrontierEntry};
use crate::mutator::{MutationConfig, ScheduleMutator};
use chaoscontrol_fault::oracle::OracleReport;
use chaoscontrol_fault::schedule::FaultSchedule;
use chaoscontrol_protocol::COVERAGE_BITMAP_ADDR;
use chaoscontrol_vmm::controller::{SimulationConfig, SimulationController};
use chaoscontrol_vmm::scheduler::SchedulingStrategy;
use chaoscontrol_vmm::vm::VmConfig;
use log::{debug, info, warn};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use thiserror::Error;

/// Errors from the exploration engine.
#[derive(Error, Debug)]
pub enum ExploreError {
    #[error("VM error: {0}")]
    Vm(#[from] chaoscontrol_vmm::vm::VmError),

    #[error("Configuration error: {0}")]
    Config(String),
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
    /// Guest physical address of coverage bitmap (0 = blind mode).
    pub coverage_gpa: u64,
    /// Optional output directory for checkpoints and reports.
    pub output_dir: Option<String>,
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
            coverage_gpa: COVERAGE_BITMAP_ADDR, // Use protocol-defined address
            output_dir: None,
        }
    }
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
        }
    }

    /// Run the full exploration loop.
    ///
    /// Returns the final report with all bugs found, coverage stats, etc.
    pub fn run(&mut self) -> Result<ExplorationReport, ExploreError> {
        info!(
            "Starting exploration: {} rounds, {} branches/round, {} VMs",
            self.config.max_rounds, self.config.branch_factor, self.config.num_vms
        );

        // Bootstrap: run initial simulation with empty schedule
        info!("Bootstrap: running initial simulation...");
        let initial_result = self.run_branch(&None, FaultSchedule::new())?;

        if let Some(snapshot) = initial_result.snapshot.clone() {
            self.add_to_frontier(snapshot, initial_result, FaultSchedule::new(), None, 0);
        }

        // Main exploration loop
        for round in 0..self.config.max_rounds {
            info!("=== Round {}/{} ===", round + 1, self.config.max_rounds);

            let round_report = self.explore_round()?;
            self.rounds_completed += 1;

            info!(
                "Round {}: {} branches, {} new edges, {} bugs, frontier: {}",
                round + 1,
                round_report.branches_run,
                round_report.new_coverage_edges,
                round_report.bugs_found,
                round_report.frontier_size
            );

            // Save checkpoint if output directory is configured
            if let Some(ref output_dir) = self.config.output_dir {
                if let Err(e) = self.save_checkpoint_to_dir(output_dir) {
                    warn!("Failed to save checkpoint: {}", e);
                }
            }

            // Check for stopping conditions
            if self.frontier.is_empty() {
                info!("Frontier exhausted, stopping early");
                break;
            }

            // Optionally stop if we found bugs (for testing)
            if round_report.bugs_found > 0 && self.config.max_rounds < 10 {
                info!("Bug found in short run, stopping");
                break;
            }
        }

        // Generate final report
        Ok(self.generate_report())
    }

    /// Execute one exploration round:
    /// 1. Select a frontier entry
    /// 2. Generate N variant fault schedules
    /// 3. For each variant: restore snapshot → apply schedule → run → collect coverage
    /// 4. Score results, add interesting ones to frontier and corpus
    /// 5. Record any bugs found
    fn explore_round(&mut self) -> Result<RoundReport, ExploreError> {
        let mut branches_run = 0;
        let mut new_coverage_edges = 0;
        let mut bugs_found = 0;

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

        // Generate variant schedules
        let variants = self.mutator.mutate(
            &base_schedule,
            self.config.branch_factor,
            &self.config.mutation,
        );

        debug!("Generated {} variant schedules", variants.len());

        // Run each variant
        for (i, schedule) in variants.into_iter().enumerate() {
            debug!("Running branch {}/{}", i + 1, self.config.branch_factor);

            let result = self.run_branch(&snapshot, schedule.clone())?;
            branches_run += 1;
            self.total_branches_run += 1;

            // Check for new coverage
            let new_edges = result
                .coverage
                .has_new_coverage(self.coverage.global_coverage());

            if new_edges > 0 {
                debug!("Branch {} found {} new edges", i + 1, new_edges);
                new_coverage_edges += new_edges;

                // Add to corpus
                self.add_to_corpus(
                    result.clone(),
                    schedule.clone(),
                    new_edges,
                    parent_depth + 1,
                );

                // Add to frontier if we got a snapshot
                if let Some(snap) = result.snapshot.clone() {
                    self.add_to_frontier(
                        snap,
                        result.clone(),
                        schedule.clone(),
                        parent_id,
                        parent_depth + 1,
                    );
                }
            }

            // Check for bugs
            let branch_bugs = self.extract_bugs(&result, &schedule);
            bugs_found += branch_bugs.len();

            if !branch_bugs.is_empty() {
                warn!("Branch {} found {} bugs!", i + 1, branch_bugs.len());
            }

            // Update global coverage
            self.coverage.update_global(&result.coverage);
        }

        Ok(RoundReport {
            branches_run,
            new_coverage_edges,
            bugs_found,
            frontier_size: self.frontier.len(),
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
        };

        self.controller = Some(SimulationController::new(sim_config)?);
        Ok(())
    }

    /// Run a single branch: restore snapshot → clear coverage → apply
    /// schedule → run for N ticks → collect coverage.
    ///
    /// Reuses the cached controller to avoid re-booting the kernel per
    /// branch (5s saved per branch).
    fn run_branch(
        &mut self,
        snapshot: &Option<chaoscontrol_vmm::controller::SimulationSnapshot>,
        schedule: FaultSchedule,
    ) -> Result<BranchResult, ExploreError> {
        self.ensure_controller()?;

        // Phase 1: restore + run (needs &mut controller)
        {
            let controller = self.controller.as_mut().unwrap();

            // Restore from snapshot if provided (rewinds VM state without reboot)
            if let Some(snap) = snapshot {
                controller.restore_all(snap)?;
            }

            // Apply the mutated fault schedule
            controller.set_schedule(schedule.clone());

            // Clear coverage bitmaps so we only see edges from THIS branch
            controller.clear_all_coverage();

            // Run for configured ticks
            controller.run(self.config.ticks_per_branch)?;
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

        let snap = controller.snapshot_all().ok();

        Ok(BranchResult {
            coverage,
            oracle_report: result_info,
            schedule,
            exit_counts: vm_exit_counts,
            halted: total_ticks >= self.config.ticks_per_branch,
            total_ticks,
            bugs: Vec::new(),
            snapshot: snap,
        })
    }

    /// Score a branch result for frontier prioritization.
    /// Factors: new coverage edges, assertion variety, depth penalty.
    fn score_branch(&self, result: &BranchResult, parent_depth: u32) -> f64 {
        let new_edges = result
            .coverage
            .has_new_coverage(self.coverage.global_coverage());
        let total_edges = result.coverage.count_bits();

        // Base score: number of new edges
        let mut score = new_edges as f64 * 10.0;

        // Bonus for high total coverage
        score += total_edges as f64 * 0.1;

        // Penalty for depth (favor shallower branches)
        let depth_penalty = (parent_depth as f64) * 0.5;
        score = (score - depth_penalty).max(0.1);

        // Bonus for assertion diversity
        let assertion_count = result.oracle_report.assertions.len();
        score += assertion_count as f64;

        score
    }

    /// Extract bug reports from a branch result.
    fn extract_bugs(&self, result: &BranchResult, schedule: &FaultSchedule) -> Vec<BugReport> {
        let mut bugs = Vec::new();

        // Check oracle for failed assertions
        for (assertion_id, record) in &result.oracle_report.assertions {
            if matches!(
                record.verdict(),
                chaoscontrol_fault::oracle::Verdict::Failed
            ) {
                bugs.push(BugReport {
                    bug_id: 0, // Will be assigned by corpus
                    assertion_id: *assertion_id as u64,
                    assertion_location: record.message.clone(),
                    schedule: schedule.clone(),
                    snapshot: result.snapshot.clone(),
                    tick: result.total_ticks,
                });
            }
        }

        bugs
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
        let score = self.score_branch(&result, depth);

        let entry = FrontierEntry {
            id: 0, // Will be assigned by frontier
            snapshot,
            coverage: result.coverage,
            score,
            times_selected: 0,
            depth,
            schedule,
            parent,
        };

        self.frontier.push(entry);
    }

    /// Add a result to the corpus.
    fn add_to_corpus(
        &mut self,
        result: BranchResult,
        schedule: FaultSchedule,
        new_edges: usize,
        depth: u32,
    ) {
        let bugs = self.extract_bugs(&result, &schedule);

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

    /// Generate pseudo-coverage from assertion variety (blind mode).
    fn assertion_coverage(&self, oracle: &OracleReport) -> CoverageBitmap {
        let mut bitmap = CoverageBitmap::new();

        // Map each assertion ID to a bitmap index
        for assertion_id in oracle.assertions.keys() {
            let index = (*assertion_id as usize) % crate::coverage::MAP_SIZE;
            bitmap.record_hit(index);
        }

        bitmap
    }

    /// Generate the final exploration report.
    fn generate_report(&self) -> ExplorationReport {
        let bugs = self.corpus.bugs();
        let coverage_stats = self.coverage.stats();
        let corpus_stats = self.corpus.stats();

        let network_stats = self
            .controller
            .as_ref()
            .map(|c| c.network().stats().clone())
            .unwrap_or_default();

        ExplorationReport {
            rounds: self.rounds_completed,
            total_branches: self.total_branches_run,
            total_edges: coverage_stats.total_edges,
            bugs,
            corpus_size: corpus_stats.total_entries,
            coverage_stats,
            network_stats,
        }
    }

    /// Get current exploration stats.
    pub fn stats(&self) -> ExplorationStats {
        ExplorationStats {
            rounds: self.rounds_completed,
            branches: self.total_branches_run,
            edges: self.coverage.stats().total_edges,
            bugs: self.corpus.bugs().len(),
            frontier_size: self.frontier.len(),
            corpus_size: self.corpus.len(),
        }
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
        let checkpoint = self.create_checkpoint();
        save_checkpoint(&checkpoint_path, &checkpoint)?;

        info!("Checkpoint saved to {}", checkpoint_path);
        Ok(())
    }

    /// Create a checkpoint from the current state.
    fn create_checkpoint(&self) -> ExplorationCheckpoint {
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
        };

        let bugs: Vec<SerializableBug> = self.corpus.bugs().iter().map(|b| b.into()).collect();

        ExplorationCheckpoint {
            config,
            global_coverage: self.coverage.global_coverage().as_slice().to_vec(),
            bugs,
            rounds_completed: self.rounds_completed,
            total_branches_run: self.total_branches_run,
            total_edges: self.coverage.stats().total_edges,
            seed: self.config.seed,
        }
    }

    /// Create an Explorer from a checkpoint, optionally overriding config fields.
    pub fn from_checkpoint(
        checkpoint: ExplorationCheckpoint,
        kernel_path_override: Option<String>,
        initrd_path_override: Option<String>,
        max_rounds_override: Option<u64>,
    ) -> Self {
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
            coverage_gpa: checkpoint.config.coverage_gpa,
            output_dir: None, // Will be set by caller if needed
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

        Self {
            config,
            frontier,
            corpus,
            mutator,
            coverage,
            rng,
            controller: None,
            rounds_completed: checkpoint.rounds_completed,
            total_branches_run: checkpoint.total_branches_run,
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
                assertions: std::collections::BTreeMap::new(),
                total_runs: 1,
                passed: 0,
                failed: 0,
                unexercised: 0,
                events: Vec::new(),
            },
            schedule: FaultSchedule::new(),
            exit_counts: vec![100],
            halted: false,
            total_ticks: 100,
            bugs: Vec::new(),
            snapshot: None,
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
            assertions: std::collections::BTreeMap::new(),
            total_runs: 1,
            passed: 0,
            failed: 0,
            unexercised: 0,
            events: Vec::new(),
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
            },
        );

        let coverage = explorer.assertion_coverage(&oracle);
        assert!(coverage.count_bits() > 0);
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
        );

        assert_eq!(restored.rounds_completed, 42);
        assert_eq!(restored.total_branches_run, 336);
        assert_eq!(restored.config.num_vms, 3);
        assert_eq!(restored.config.seed, 12345);
        assert_eq!(restored.coverage.stats().total_edges, 100);

        // Cleanup
        let _ = fs::remove_dir_all(&checkpoint_dir);
    }
}
