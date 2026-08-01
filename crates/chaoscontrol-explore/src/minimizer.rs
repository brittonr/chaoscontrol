//! Fault schedule minimization via delta debugging.
//!
//! When the explorer finds a bug, the triggering fault schedule may contain
//! many faults irrelevant to the failure.  The minimizer systematically
//! removes faults and re-runs the branch to find the smallest schedule
//! that still triggers the same assertion failure.
//!
//! # Algorithm
//!
//! Uses Zeller's delta debugging (ddmin):
//!
//! 1. Partition the schedule into N chunks
//! 2. Try removing each chunk — if the bug persists, keep the removal
//! 3. Try each chunk alone (complement removal) — if the bug persists,
//!    keep only that chunk's complement
//! 4. If neither worked, double N and retry with smaller chunks
//! 5. Stop when N equals the schedule length (single-fault granularity)
//!
//! # Example
//!
//! ```no_run
//! use chaoscontrol_explore::minimizer::{Minimizer, MinimizeConfig};
//! use chaoscontrol_explore::corpus::BugReport;
//!
//! // bug_report obtained from exploration
//! // let minimizer = Minimizer::new(config, bug_report);
//! // let minimized = minimizer.minimize()?;
//! // assert!(minimized.schedule.total() <= bug_report.schedule.total());
//! ```

use crate::corpus::BugReport;
use crate::explorer::ExploreError;
use chaoscontrol_fault::oracle::Verdict;
use chaoscontrol_fault::schedule::FaultSchedule;
use chaoscontrol_vmm::controller::{SimulationConfig, SimulationController, SimulationSnapshot};
use chaoscontrol_vmm::scheduler::SchedulingStrategy;
use chaoscontrol_vmm::vm::VmConfig;
use log::{debug, info};

/// Configuration for the minimizer.
#[derive(Clone)]
pub struct MinimizeConfig {
    /// Number of VMs per simulation.
    pub num_vms: usize,
    /// Per-VM config.
    pub vm_config: VmConfig,
    /// Kernel path.
    pub kernel_path: String,
    /// Optional initrd.
    pub initrd_path: Option<String>,
    /// Master seed (must match the exploration run).
    pub seed: u64,
    /// Exits per VM per scheduling round.
    pub quantum: u64,
    /// Scheduling strategy for SMP.
    pub scheduling_strategy: SchedulingStrategy,
    /// How many ticks to run each candidate branch.
    pub ticks_per_branch: u64,
    /// Optional disk image path.
    pub disk_image_path: Option<String>,
    /// Max bootstrap ticks.
    pub bootstrap_budget: u64,
    /// Guest physical address of coverage bitmap.
    pub coverage_gpa: u64,
}

/// Result of a minimization run.
#[derive(Debug, Clone)]
pub struct MinimizeResult {
    /// The minimized schedule.
    pub schedule: FaultSchedule,
    /// Original fault count.
    pub original_faults: usize,
    /// Minimized fault count.
    pub minimized_faults: usize,
    /// Total candidate schedules tested.
    pub candidates_tested: usize,
    /// The assertion that was preserved.
    pub assertion_id: u64,
}

/// Fault schedule minimizer.
///
/// Takes a bug-triggering schedule and produces the smallest sub-schedule
/// that still triggers the same assertion failure.
pub struct Minimizer {
    config: MinimizeConfig,
    bug: BugReport,
    controller: Option<SimulationController>,
    candidates_tested: usize,
}

impl Minimizer {
    /// Create a new minimizer for the given bug report.
    pub fn new(config: MinimizeConfig, bug: BugReport) -> Self {
        Self {
            config,
            bug,
            controller: None,
            candidates_tested: 0,
        }
    }

    /// Run delta debugging to minimize the fault schedule.
    ///
    /// Returns the smallest schedule that still triggers the same
    /// assertion failure, along with stats about the minimization.
    pub fn minimize(&mut self) -> Result<MinimizeResult, ExploreError> {
        let original_total = self.bug.schedule.total();

        if original_total == 0 {
            return Err(ExploreError::Config {
                message: "cannot minimize an empty fault schedule".to_string(),
            });
        }

        info!(
            "Minimizing schedule: {} faults, assertion {}",
            original_total, self.bug.assertion_id
        );

        // Bootstrap the controller, unless the bug report already carries
        // the parent snapshot that started the failing branch.
        self.ensure_controller()?;
        let replay_snapshot = self.replay_start_snapshot()?;

        // Verify the full schedule actually triggers the bug
        let all_indices: Vec<usize> = (0..original_total).collect();
        if !self.triggers_bug(&replay_snapshot, &all_indices)? {
            info!("Full schedule does not trigger bug — cannot minimize");
            return Ok(MinimizeResult {
                schedule: self.bug.schedule.clone(),
                original_faults: original_total,
                minimized_faults: original_total,
                candidates_tested: self.candidates_tested,
                assertion_id: self.bug.assertion_id,
            });
        }

        info!(
            "Confirmed: full schedule triggers assertion {}",
            self.bug.assertion_id
        );

        // Delta debugging (ddmin)
        let minimized_indices = self.ddmin(&replay_snapshot, all_indices)?;

        let minimized_schedule = self
            .bug
            .schedule
            .subset(&minimized_indices)
            .map_err(|error| ExploreError::Config {
                message: error.to_string(),
            })?;
        let minimized_count = minimized_schedule.total();

        info!(
            "Minimization complete: {} → {} faults ({} candidates tested)",
            original_total, minimized_count, self.candidates_tested
        );

        Ok(MinimizeResult {
            schedule: minimized_schedule,
            original_faults: original_total,
            minimized_faults: minimized_count,
            candidates_tested: self.candidates_tested,
            assertion_id: self.bug.assertion_id,
        })
    }

    /// Zeller's ddmin algorithm.
    ///
    /// `indices` is the current set of fault indices known to trigger the bug.
    /// Returns the minimal subset.
    fn ddmin(
        &mut self,
        snapshot: &SimulationSnapshot,
        mut indices: Vec<usize>,
    ) -> Result<Vec<usize>, ExploreError> {
        let mut n = 2usize; // Start with 2 chunks

        while indices.len() >= 2 {
            let chunk_size = indices.len().div_ceil(n);
            let chunks: Vec<Vec<usize>> = indices.chunks(chunk_size).map(|c| c.to_vec()).collect();
            let num_chunks = chunks.len();

            debug!(
                "ddmin: {} faults, {} chunks of ~{}",
                indices.len(),
                num_chunks,
                chunk_size
            );

            let mut reduced = false;

            // Try removing each chunk
            for (i, chunk) in chunks.iter().enumerate() {
                let complement: Vec<usize> = indices
                    .iter()
                    .filter(|idx| !chunk.contains(idx))
                    .copied()
                    .collect();

                if complement.is_empty() {
                    continue;
                }

                debug!(
                    "  trying without chunk {} ({} faults → {})",
                    i,
                    indices.len(),
                    complement.len()
                );

                if self.triggers_bug(snapshot, &complement)? {
                    info!(
                        "  removed chunk {} ({} faults): {} → {}",
                        i,
                        chunk.len(),
                        indices.len(),
                        complement.len()
                    );
                    indices = complement;
                    // Restart with n=2 on the smaller set
                    n = 2.max(n - 1);
                    reduced = true;
                    break;
                }
            }

            if reduced {
                continue;
            }

            // Try each chunk alone (i.e., remove its complement)
            for (i, chunk) in chunks.iter().enumerate() {
                if chunk.len() == indices.len() {
                    continue; // Skip if chunk IS the whole set
                }

                debug!("  trying chunk {} alone ({} faults)", i, chunk.len());

                if self.triggers_bug(snapshot, chunk)? {
                    info!(
                        "  chunk {} alone triggers bug: {} → {}",
                        i,
                        indices.len(),
                        chunk.len()
                    );
                    indices = chunk.clone();
                    n = 2;
                    reduced = true;
                    break;
                }
            }

            if reduced {
                continue;
            }

            // Neither worked — increase granularity
            if n >= indices.len() {
                break; // Already at single-fault granularity
            }

            n = (2 * n).min(indices.len());
            debug!("  increasing granularity to {} chunks", n);
        }

        Ok(indices)
    }

    /// Test whether a subset of faults (by index) triggers the target bug.
    fn triggers_bug(
        &mut self,
        snapshot: &SimulationSnapshot,
        indices: &[usize],
    ) -> Result<bool, ExploreError> {
        let candidates_tested =
            self.candidates_tested
                .checked_add(1)
                .ok_or_else(|| ExploreError::Config {
                    message: "minimizer candidate counter overflow".to_string(),
                })?;
        let schedule = self
            .bug
            .schedule
            .subset(indices)
            .map_err(|error| ExploreError::Config {
                message: error.to_string(),
            })?;
        crate::bug::identity::validate_carrier(
            self.bug.assertion_id,
            Some(&self.bug.assertion_identity),
        )
        .map_err(|error| ExploreError::Config {
            message: error.to_string(),
        })?;
        snapshot
            .validate_assertion_evidence(self.config.num_vms, &self.bug.assertion_identity)
            .map_err(|error| ExploreError::Config { message: error })?;
        self.candidates_tested = candidates_tested;
        let controller = self
            .controller
            .as_mut()
            .ok_or_else(|| ExploreError::Config {
                message: "minimizer controller is not initialized".to_string(),
            })?;

        // Restore only after the snapshot admits the exact assertion target.
        controller.restore_all(snapshot)?;
        let restored_report = controller.report();
        crate::bug::identity::resolve_restored_report(
            self.bug.assertion_id,
            Some(&self.bug.assertion_identity),
            &restored_report,
        )
        .map_err(|error| ExploreError::Config {
            message: error.to_string(),
        })?;
        controller.reset_vm_statuses();

        // Apply candidate schedule
        controller.set_schedule(schedule);
        controller.clear_all_coverage();

        // Run
        controller.run(self.config.ticks_per_branch)?;

        // Check if the exact same assertion failed.
        let report = controller.report();
        let record = crate::bug::identity::resolve_restored_report(
            self.bug.assertion_id,
            Some(&self.bug.assertion_identity),
            &report,
        )
        .map_err(|error| ExploreError::Config {
            message: error.to_string(),
        })?;
        let triggered = matches!(record.verdict(), Verdict::Failed);

        debug!(
            "  candidate {} ({} faults): {}",
            self.candidates_tested,
            indices.len(),
            if triggered { "TRIGGERS BUG" } else { "no bug" }
        );

        Ok(triggered)
    }

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
            dlog_dir: None,
            bootstrap_budget: None,
        };

        self.controller = Some(SimulationController::new(sim_config)?);
        Ok(())
    }

    fn bootstrap(&mut self) -> Result<SimulationSnapshot, ExploreError> {
        let controller = self.controller.as_mut().unwrap();
        controller.set_schedule(FaultSchedule::new());
        controller.clear_all_coverage();
        controller.run_until_setup_complete(self.config.bootstrap_budget)?;

        let snapshot = controller.snapshot_all()?;

        info!("Minimizer bootstrap complete at tick {}", controller.tick());

        Ok(snapshot)
    }

    fn replay_start_snapshot(&mut self) -> Result<SimulationSnapshot, ExploreError> {
        if let Some(snapshot) = self.bug.snapshot.clone() {
            info!(
                "Using saved replay parent snapshot at depth {}",
                self.bug.replay_parent_depth
            );
            return Ok(snapshot);
        }

        self.bootstrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chaoscontrol_fault::faults::Fault;
    use chaoscontrol_fault::schedule::ScheduledFault;
    use rand::SeedableRng;

    fn test_config() -> MinimizeConfig {
        MinimizeConfig {
            num_vms: 3,
            vm_config: VmConfig::default(),
            kernel_path: "vmlinux".into(),
            initrd_path: None,
            seed: 42,
            quantum: 100,
            scheduling_strategy: SchedulingStrategy::RoundRobin,
            ticks_per_branch: 1000,
            disk_image_path: None,
            bootstrap_budget: 10_000,
            coverage_gpa: 0xE0000,
        }
    }

    fn dummy_snapshot(tick: u64) -> SimulationSnapshot {
        let network_state = chaoscontrol_vmm::controller::NetworkFabric {
            partitions: Vec::new(),
            latency: vec![0, 0],
            jitter: vec![0, 0],
            bandwidth_bps: vec![0, 0],
            next_free_tick: vec![0, 0],
            in_flight: Vec::new(),
            packet_in_flight: Vec::new(),
            loss_rate_ppm: Vec::new(),
            corruption_rate_ppm: Vec::new(),
            reorder_window: Vec::new(),
            duplicate_rate_ppm: Vec::new(),
            rng: rand_chacha::ChaCha20Rng::seed_from_u64(42),
            stats: Default::default(),
        };
        let engine = chaoscontrol_fault::engine::FaultEngine::new(Default::default());

        SimulationSnapshot {
            tick,
            vm_snapshots: Vec::new(),
            network_state,
            fault_engine_snapshot: engine.snapshot(),
            vcpu_stall_until: vec![],
            clock_freeze: vec![],
            clock_jitter_bound: vec![],
        }
    }

    fn bug_with_snapshot(snapshot: Option<SimulationSnapshot>) -> BugReport {
        BugReport {
            bug_id: 7,
            assertion_id: 42,
            assertion_identity: crate::test_support::assertion_identity(42),
            assertion_location: "assertion".into(),
            schedule: FaultSchedule::new(),
            snapshot,
            tick: 123,
            replay_parent_depth: 2,
            replay_parent_snapshot_ref: None,
            dedup_key: 0,
            schedule_variant: None,
            scenario_config: None,
            scenario_summary: None,
        }
    }

    #[test]
    fn test_minimize_config_defaults() {
        let config = test_config();
        assert_eq!(config.num_vms, 3);
        assert_eq!(config.seed, 42);
    }

    #[test]
    fn replay_start_snapshot_prefers_saved_parent_snapshot() {
        let expected = dummy_snapshot(99);
        let mut minimizer = Minimizer::new(test_config(), bug_with_snapshot(Some(expected)));

        let replay_snapshot = minimizer.replay_start_snapshot().unwrap();

        assert_eq!(replay_snapshot.tick, 99);
        assert!(minimizer.controller.is_none());
    }

    #[test]
    fn rejects_empty_schedule_before_controller_creation() {
        let mut minimizer = Minimizer::new(test_config(), bug_with_snapshot(None));

        let error = minimizer
            .minimize()
            .expect_err("empty schedule must not produce a minimized bug");

        assert!(matches!(error, ExploreError::Config { .. }));
        assert!(minimizer.controller.is_none());
    }

    #[test]
    fn test_schedule_subset() {
        let mut schedule = FaultSchedule::new();
        schedule.add(ScheduledFault::new(100, Fault::NetworkHeal));
        schedule.add(ScheduledFault::new(200, Fault::ProcessKill { target: 0 }));
        schedule.add(ScheduledFault::new(300, Fault::DiskFull { target: 1 }));

        // Take only indices 0 and 2
        let sub = schedule.subset(&[0, 2]).expect("valid subset");
        assert_eq!(sub.total(), 2);

        // Verify the faults are correct
        let faults = sub.faults();
        assert_eq!(faults[0].time_ns, 100);
        assert_eq!(faults[1].time_ns, 300);
    }

    #[test]
    fn test_schedule_subset_empty() {
        let mut schedule = FaultSchedule::new();
        schedule.add(ScheduledFault::new(100, Fault::NetworkHeal));

        let sub = schedule.subset(&[]).expect("empty subset is valid");
        assert_eq!(sub.total(), 0);
    }

    #[test]
    fn test_schedule_subset_out_of_bounds() {
        const INVALID_INDEX: usize = 5;
        let mut schedule = FaultSchedule::new();
        schedule.add(ScheduledFault::new(100, Fault::NetworkHeal));

        let error = schedule
            .subset(&[0, INVALID_INDEX])
            .expect_err("out-of-bounds index must fail closed");
        assert_eq!(
            error,
            chaoscontrol_fault::schedule::ScheduleSubsetError::OutOfBounds {
                index: INVALID_INDEX,
                length: 1,
            }
        );
    }

    #[test]
    fn test_schedule_subset_rejects_duplicate_indices() {
        let mut schedule = FaultSchedule::new();
        schedule.add(ScheduledFault::new(100, Fault::NetworkHeal));

        let error = schedule
            .subset(&[0, 0])
            .expect_err("duplicate index must fail closed");
        assert_eq!(
            error,
            chaoscontrol_fault::schedule::ScheduleSubsetError::DuplicateIndex { index: 0 }
        );
    }

    #[test]
    fn test_schedule_faults_accessor() {
        let mut schedule = FaultSchedule::new();
        schedule.add(ScheduledFault::new(200, Fault::ProcessKill { target: 1 }));
        schedule.add(ScheduledFault::new(100, Fault::NetworkHeal));

        let faults = schedule.faults();
        assert_eq!(faults.len(), 2);
        // Sorted by time
        assert_eq!(faults[0].time_ns, 100);
        assert_eq!(faults[1].time_ns, 200);
    }

    #[test]
    fn test_minimize_result_reduction_ratio() {
        let result = MinimizeResult {
            schedule: FaultSchedule::new(),
            original_faults: 20,
            minimized_faults: 3,
            candidates_tested: 47,
            assertion_id: 42,
        };

        let ratio = result.minimized_faults as f64 / result.original_faults as f64;
        assert!(ratio < 0.2); // 85% reduction
    }
}
