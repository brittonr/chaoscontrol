//! Worker pool for parallel branch execution.
//!
//! Each worker owns a [`SimulationController`] with independent KVM VM
//! file descriptors. Workers are bootstrapped once (kernel boot +
//! `setup_complete`), then reused across rounds via snapshot restore.

use crate::branch_recovery::{
    decide_after_watchdog, host_watchdog_vcpu, BranchOutcome, RecoverySnapshot, WatchdogDecision,
    MAX_BRANCH_WATCHDOG_ATTEMPTS,
};
use crate::coverage::{CoverageBitmap, CoverageCollector};
use crate::explorer::{BranchResult, BranchTimings, ExplorerConfig};
use chaoscontrol_fault::schedule::FaultSchedule;
use chaoscontrol_vmm::controller::{SimulationConfig, SimulationController, SimulationSnapshot};
use chaoscontrol_vmm::scheduler::ScheduleVariant;
use chaoscontrol_vmm::vm::VmError;
use log::{debug, info};
use std::sync::Arc;

/// A unit of work dispatched to a worker thread.
#[derive(Clone)]
pub struct BranchWork {
    /// Fault schedule to apply.
    pub schedule: FaultSchedule,
    /// Branch index (for deterministic result ordering).
    pub branch_index: usize,
    /// Optional schedule variant for vCPU interleaving diversity.
    pub schedule_variant: Option<ScheduleVariant>,
}

/// Pool of worker controllers for parallel branch execution.
///
/// Each worker owns a `SimulationController` created and bootstrapped
/// on construction. Workers are reused across rounds — they restore
/// from a shared snapshot for each branch instead of rebooting.
///
/// **Signal safety:** Deterministic SMP does not arm `SIGALRM`.
/// Single-vCPU watchdog timers use `SIGEV_THREAD_ID` in parallel workers.
/// Optional PMU overflow delivery uses `F_OWNER_TID` for the worker thread.
pub struct WorkerPool {
    /// One controller per worker. Workers are indexed 0..num_workers.
    /// Using `Option` so we can `take()` controllers into threads.
    controllers: Vec<Option<SimulationController>>,
    /// Explorer config for coverage collection settings.
    coverage_gpa: u64,
    /// Ticks per branch.
    ticks_per_branch: u64,
    /// Per-VM base memory images for incremental snapshots.
    memory_bases: Vec<Arc<Vec<u8>>>,
    /// Simulation configuration each worker controller was built from.
    sim_config: SimulationConfig,
    /// Bootstrap state used to replace a controller poisoned by a branch watchdog.
    recovery: Option<RecoverySnapshot>,
}

impl WorkerPool {
    /// Create a new worker pool with `num_workers` controllers.
    ///
    /// Each controller is created and bootstrapped (kernel boot +
    /// `setup_complete`) in parallel. This takes ~87s per worker
    /// but runs concurrently, so wall-clock time is ~87s regardless
    /// of worker count.
    pub fn new(
        config: &ExplorerConfig,
        num_workers: usize,
    ) -> Result<Self, chaoscontrol_vmm::vm::VmError> {
        assert!(num_workers >= 1, "need at least 1 worker");

        info!(
            "Bootstrapping {} worker controllers in parallel...",
            num_workers
        );

        let sim_config = worker_simulation_config(config);

        // Build controllers in parallel using scoped threads.
        // Each thread creates + bootstraps its own controller.
        let raw_handles: Vec<_> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..num_workers)
                .map(|worker_id| {
                    let sim_config = sim_config.clone();
                    let bootstrap_budget = config.bootstrap_budget;
                    s.spawn(
                        move || -> Result<SimulationController, chaoscontrol_vmm::vm::VmError> {
                            let mut ctrl = SimulationController::new(sim_config)?;
                            ctrl.set_schedule(FaultSchedule::new())?;
                            ctrl.clear_all_coverage();
                            ctrl.run_until_setup_complete(bootstrap_budget)?;

                            info!("Worker {} bootstrapped", worker_id);
                            Ok(ctrl)
                        },
                    )
                })
                .collect();

            handles
                .into_iter()
                .enumerate()
                .map(|(id, h)| match h.join() {
                    Ok(result) => Some((id, result)),
                    Err(panic_payload) => {
                        let msg = panic_message(&panic_payload);
                        log::error!("Worker {} bootstrap panicked: {}", id, msg);
                        None
                    }
                })
                .collect::<Vec<_>>()
        });

        // Collect results. Panicked workers are skipped; errors propagated.
        let mut pool_controllers = Vec::with_capacity(num_workers);
        for entry in raw_handles.into_iter().flatten() {
            let (worker_id, result) = entry;
            match result {
                Ok(ctrl) => pool_controllers.push(Some(ctrl)),
                Err(e) => {
                    log::warn!("Worker {} bootstrap failed: {}", worker_id, e);
                }
            }
        }

        if pool_controllers.is_empty() {
            return Err(chaoscontrol_vmm::vm::VmError::Snapshot {
                message: "all worker bootstraps failed".into(),
            });
        }

        info!("{}/{} workers ready", pool_controllers.len(), num_workers);

        Ok(Self {
            controllers: pool_controllers,
            coverage_gpa: config.coverage_gpa,
            ticks_per_branch: config.ticks_per_branch,
            memory_bases: Vec::new(),
            sim_config,
            recovery: None,
        })
    }

    /// Number of workers in the pool.
    pub fn num_workers(&self) -> usize {
        self.controllers.len()
    }

    /// Set per-VM base memory images for incremental snapshots.
    pub fn set_memory_bases(&mut self, bases: Vec<Arc<Vec<u8>>>) {
        self.memory_bases = bases;
    }

    /// Retain bootstrap state so workers can replace watchdog-poisoned controllers.
    pub fn set_recovery(&mut self, recovery: RecoverySnapshot) {
        self.recovery = Some(recovery);
    }

    /// Run a batch of branches in parallel across workers.
    ///
    /// Each worker restores the snapshot, applies its assigned schedule,
    /// runs for `ticks_per_branch` ticks, captures a snapshot, and
    /// returns a [`BranchOutcome`].
    ///
    /// Results are returned in branch-index order (deterministic
    /// regardless of which worker finishes first).
    pub fn run_branches(
        &mut self,
        snapshot: &SimulationSnapshot,
        work: Vec<BranchWork>,
    ) -> Result<Vec<BranchOutcome>, chaoscontrol_vmm::vm::VmError> {
        if work.is_empty() {
            return Ok(Vec::new());
        }

        let num_workers = self.controllers.len();
        let num_branches = work.len();

        debug!(
            "Dispatching {} branches across {} workers",
            num_branches, num_workers
        );

        // Take controllers out of the pool so we can move them into threads.
        let mut taken: Vec<Option<SimulationController>> =
            self.controllers.iter_mut().map(|c| c.take()).collect();

        // Split work into chunks, one per worker.
        let chunk_size = num_branches.div_ceil(num_workers);
        let work_chunks: Vec<Vec<BranchWork>> = work
            .into_iter()
            .collect::<Vec<_>>()
            .chunks(chunk_size)
            .map(|c| c.to_vec())
            .collect();

        let params = BranchRunParams {
            snapshot,
            coverage_gpa: self.coverage_gpa,
            ticks_per_branch: self.ticks_per_branch,
            use_incremental: !self.memory_bases.is_empty(),
        };
        let recovery = WorkerRecovery {
            snapshot: self.recovery.as_ref(),
            sim_config: &self.sim_config,
        };
        let memory_bases = &self.memory_bases;

        // Run all chunks in parallel.
        let chunk_results: Vec<Result<(SimulationController, Vec<BranchOutcome>), _>> =
            std::thread::scope(|s| {
                let handles: Vec<_> = work_chunks
                    .into_iter()
                    .enumerate()
                    .map(|(worker_idx, chunk)| {
                        let mut ctrl = taken[worker_idx].take().expect("controller already taken");
                        let params = &params;
                        let recovery = &recovery;
                        let bases = memory_bases;

                        s.spawn(move || -> Result<(SimulationController, Vec<BranchOutcome>), chaoscontrol_vmm::vm::VmError> {
                            // Initialize per-thread POSIX timers so the
                            // single-vCPU watchdog SIGALRM targets this
                            // thread, not the process.
                            ctrl.init_thread_timers()?;

                            // Set bases for incremental snapshots on this worker's controller.
                            if !bases.is_empty() {
                                ctrl.set_memory_bases(bases.clone());
                            }

                            let mut outcomes = Vec::with_capacity(chunk.len());

                            for (branch_offset, item) in chunk.iter().enumerate() {
                                let outcome = run_branch_with_recovery(
                                    &mut ctrl,
                                    item,
                                    params,
                                    recovery,
                                    worker_idx,
                                    branch_offset,
                                )?;
                                outcomes.push(outcome);
                            }

                            Ok((ctrl, outcomes))
                        })
                    })
                    .collect();

                handles
                    .into_iter()
                    .map(|h| h.join().expect("worker thread poisoned"))
                    .collect()
            });

        // Put controllers back and collect results.
        let mut all_results: Vec<(usize, BranchOutcome)> = Vec::with_capacity(num_branches);
        let mut branch_offset = 0;

        for (worker_idx, chunk_result) in chunk_results.into_iter().enumerate() {
            let (ctrl, branch_results) = chunk_result?;
            self.controllers[worker_idx] = Some(ctrl);

            for result in branch_results {
                all_results.push((branch_offset, result));
                branch_offset += 1;
            }
        }

        // Sort by original branch index for deterministic ordering.
        all_results.sort_by_key(|(idx, _)| *idx);
        let results = all_results.into_iter().map(|(_, r)| r).collect();

        Ok(results)
    }
}

/// Branch inputs shared by every branch in one pool dispatch.
struct BranchRunParams<'a> {
    snapshot: &'a SimulationSnapshot,
    coverage_gpa: u64,
    ticks_per_branch: u64,
    use_incremental: bool,
}

/// What a worker needs to replace a watchdog-poisoned controller.
struct WorkerRecovery<'a> {
    snapshot: Option<&'a RecoverySnapshot>,
    sim_config: &'a SimulationConfig,
}

/// Simulation configuration for worker controllers.
fn worker_simulation_config(config: &ExplorerConfig) -> SimulationConfig {
    let mut vm_config = config.vm_config.clone();
    vm_config.scheduling_strategy = config.scheduling_strategy;

    SimulationConfig {
        num_vms: config.num_vms,
        vm_config,
        kernel_path: config.kernel_path.clone(),
        initrd_path: config.initrd_path.clone(),
        seed: config.seed,
        quantum: config.quantum,
        schedule: FaultSchedule::new(),
        disk_image_path: config.disk_image_path.clone(),
        base_core: None,
        dlog_dir: None,
        bootstrap_budget: None,
    }
}

/// Run one branch, replacing the worker controller after host watchdog timeouts.
///
/// Watchdog timeouts retry or discard the branch. Other errors and panics keep
/// the zero-coverage placeholder result.
fn run_branch_with_recovery(
    ctrl: &mut SimulationController,
    item: &BranchWork,
    params: &BranchRunParams<'_>,
    recovery: &WorkerRecovery<'_>,
    worker_idx: usize,
    branch_offset: usize,
) -> Result<BranchOutcome, VmError> {
    let mut watchdog_firings: u32 = 0;
    loop {
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_single_branch(
                ctrl,
                params.snapshot,
                item.schedule.clone(),
                item.schedule_variant.as_ref(),
                params.coverage_gpa,
                params.ticks_per_branch,
                params.use_incremental,
            )
        }));
        let error = match attempt {
            Ok(Ok(result)) => {
                return Ok(BranchOutcome::Completed {
                    result,
                    watchdog_firings,
                })
            }
            Ok(Err(error)) => error,
            Err(panic_payload) => {
                let msg = panic_message(&panic_payload);
                log::error!(
                    "Worker {} branch {} panicked: {}",
                    worker_idx,
                    branch_offset,
                    msg
                );
                return Ok(BranchOutcome::Completed {
                    result: empty_branch_result(item),
                    watchdog_firings,
                });
            }
        };
        let Some(vcpu) = host_watchdog_vcpu(&error) else {
            if item.schedule_variant.is_some() {
                return Err(error);
            }
            log::error!(
                "Worker {} branch {} failed: {}",
                worker_idx,
                branch_offset,
                error
            );
            return Ok(BranchOutcome::Completed {
                result: empty_branch_result(item),
                watchdog_firings,
            });
        };
        watchdog_firings = watchdog_firings.saturating_add(1);
        let Some(snapshot) = recovery.snapshot else {
            // Without retained bootstrap state the poisoned controller cannot be replaced.
            return Ok(BranchOutcome::Discarded {
                vcpu,
                watchdog_firings,
            });
        };
        let decision = decide_after_watchdog(watchdog_firings, true);
        log::warn!(
            "Worker {} branch {}: host watchdog interrupted vCPU {} (attempt {}/{}); replacing controller, {:?}",
            worker_idx,
            branch_offset,
            vcpu,
            watchdog_firings,
            MAX_BRANCH_WATCHDOG_ATTEMPTS,
            decision
        );
        *ctrl = snapshot.rebuild_controller(recovery.sim_config.clone())?;
        ctrl.init_thread_timers()?;
        if decision == WatchdogDecision::Discard {
            return Ok(BranchOutcome::Discarded {
                vcpu,
                watchdog_firings,
            });
        }
    }
}

/// Run a single branch on a controller.
///
/// Restores snapshot → applies schedule → runs → captures snapshot → returns result.
fn run_single_branch(
    controller: &mut SimulationController,
    snapshot: &SimulationSnapshot,
    schedule: FaultSchedule,
    schedule_variant: Option<&ScheduleVariant>,
    coverage_gpa: u64,
    ticks_per_branch: u64,
    use_incremental: bool,
) -> Result<BranchResult, chaoscontrol_vmm::vm::VmError> {
    // Restore — use incremental path when bases are available.
    if use_incremental {
        controller.restore_all_incremental(snapshot)?;
    } else {
        controller.restore_all(snapshot)?;
    }
    controller.reset_vm_statuses();

    // Apply schedule variant (vCPU interleaving diversity)
    if let Some(variant) = schedule_variant {
        controller.apply_schedule_variant(variant)?;
    }

    // Apply the schedule in a new branch run.
    controller.begin_counterfactual_fault_run(schedule.clone())?;

    // Clear coverage
    controller.clear_all_coverage();

    // Run
    controller.run(ticks_per_branch)?;

    // Collect results
    let result_info = controller.report();
    let vm_exit_counts: Vec<u64> = (0..controller.num_vms())
        .map(|i| controller.vm_slot(i).map_or(0, |s| s.vm.exit_count()))
        .collect();
    let total_ticks = controller.tick();

    // Collect coverage
    let coverage = if coverage_gpa != 0 && controller.num_vms() > 0 {
        if let Some(vm_slot) = controller.vm_slot(0) {
            let mut collector = CoverageCollector::new(coverage_gpa);
            collector.collect_from_guest(vm_slot.vm.memory().inner())
        } else {
            CoverageBitmap::new()
        }
    } else {
        // Blind mode: derive coverage from assertion variety
        let mut bitmap = CoverageBitmap::new();
        for assertion_id in result_info.assertions.keys() {
            let index = (*assertion_id as usize) % crate::coverage::MAP_SIZE;
            bitmap.record_hit(index);
        }
        bitmap
    };

    // Capture snapshot
    let snap = if use_incremental {
        controller
            .snapshot_all_incremental()
            .ok()
            .map(|(s, dirty)| {
                debug!("Worker incremental snapshot: {} dirty pages", dirty);
                s
            })
    } else {
        controller.snapshot_all().ok()
    };

    let schedule_fingerprint = controller.schedule_fingerprint();

    Ok(BranchResult {
        coverage,
        oracle_report: result_info,
        schedule,
        exit_counts: vm_exit_counts,
        halted: total_ticks >= ticks_per_branch,
        total_ticks,
        bugs: Vec::new(),
        snapshot: snap,
        schedule_variant: schedule_variant.cloned(),
        schedule_fingerprint,
        timings: BranchTimings::default(),
    })
}

/// Create a zero-coverage placeholder result for a panicked/failed branch.
fn empty_branch_result(work: &BranchWork) -> BranchResult {
    BranchResult {
        coverage: CoverageBitmap::new(),
        oracle_report: chaoscontrol_fault::oracle::OracleReport::empty(),
        schedule: work.schedule.clone(),
        exit_counts: Vec::new(),
        halted: false,
        total_ticks: 0,
        bugs: Vec::new(),
        snapshot: None,
        schedule_variant: work.schedule_variant.clone(),
        schedule_fingerprint: 0,
        timings: BranchTimings::default(),
    }
}

/// Extract a human-readable message from a panic payload.
fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_work_fields() {
        let work = BranchWork {
            schedule: FaultSchedule::new(),
            branch_index: 42,
            schedule_variant: None,
        };
        assert_eq!(work.branch_index, 42);
    }
}
