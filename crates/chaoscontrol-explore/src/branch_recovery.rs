//! Host watchdog recovery for exploration branches.
//!
//! A single-vCPU branch run can expire the host wall-clock watchdog when the
//! host deschedules the vCPU thread. The VMM reports that as an operational
//! `HostWatchdogTimeout` and permanently poisons the controller round, so the
//! controller cannot run another branch.
//!
//! The pure part of this module classifies branch errors, decides whether to
//! retry, and keeps the per-round ledger. The shell part, [`RecoverySnapshot`],
//! builds a replacement controller from the retained bootstrap state.

use crate::explorer::{BranchResult, ExploreError};
use chaoscontrol_vmm::controller::{SimulationConfig, SimulationController, SimulationSnapshot};
use chaoscontrol_vmm::snapshot::SnapshotMemory;
use chaoscontrol_vmm::vm::VmError;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Attempts per branch before a watchdog-interrupted branch is discarded.
///
/// Two retries absorb transient host descheduling. The bound caps recovery
/// work for a guest that never reaches a VM exit; each attempt stops at the
/// single-vCPU host watchdog.
pub const MAX_BRANCH_WATCHDOG_ATTEMPTS: u32 = 3;

/// Return the interrupted vCPU when `error` is a host watchdog timeout.
pub fn host_watchdog_vcpu(error: &VmError) -> Option<usize> {
    match error {
        VmError::HostWatchdogTimeout { vcpu } => Some(*vcpu),
        _ => None,
    }
}

/// Return the interrupted vCPU when an explorer error wraps a host watchdog timeout.
pub fn explore_error_watchdog_vcpu(error: &ExploreError) -> Option<usize> {
    match error {
        ExploreError::Vm { source } => host_watchdog_vcpu(source),
        _ => None,
    }
}

/// What to do with a branch after a watchdog timeout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchdogDecision {
    /// Run the branch again from its parent snapshot on a replacement controller.
    Retry,
    /// Drop the branch; it contributes no exploration evidence.
    Discard,
}

/// Decide the next step after `attempts_made` watchdog-interrupted attempts.
///
/// A branch without a parent snapshot started from mutable controller state
/// that a replacement controller cannot reproduce, so it is never retried.
pub fn decide_after_watchdog(
    attempts_made: u32,
    parent_snapshot_available: bool,
) -> WatchdogDecision {
    if parent_snapshot_available && attempts_made < MAX_BRANCH_WATCHDOG_ATTEMPTS {
        WatchdogDecision::Retry
    } else {
        WatchdogDecision::Discard
    }
}

/// Final outcome of one branch after watchdog recovery.
#[allow(
    clippy::large_enum_variant,
    reason = "Discarded is rare; boxing would allocate for every completed branch"
)]
pub enum BranchOutcome {
    /// The branch produced a result, possibly after watchdog retries.
    Completed {
        result: BranchResult,
        watchdog_firings: u32,
    },
    /// Every permitted attempt hit the host watchdog.
    Discarded { vcpu: usize, watchdog_firings: u32 },
}

impl BranchOutcome {
    /// Wrap a result that never hit the watchdog.
    pub fn completed(result: BranchResult) -> Self {
        Self::Completed {
            result,
            watchdog_firings: 0,
        }
    }
}

/// Per-round watchdog accounting and the all-discard guard.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RoundWatchdogLedger {
    /// Branches attempted this round (each counted once, however many retries).
    pub attempted_branches: usize,
    /// Branches that produced a result.
    pub completed_branches: usize,
    /// Host watchdog firings, including firings recovered by retry.
    pub watchdog_firings: usize,
    /// Branches dropped after exhausting their attempts.
    pub discarded_branches: usize,
    last_discarded_vcpu: Option<usize>,
}

impl RoundWatchdogLedger {
    /// Record one branch outcome and return its result if it completed.
    pub fn admit(&mut self, outcome: BranchOutcome) -> Option<BranchResult> {
        self.attempted_branches = self.attempted_branches.saturating_add(1);
        match outcome {
            BranchOutcome::Completed {
                result,
                watchdog_firings,
            } => {
                self.add_firings(watchdog_firings);
                self.completed_branches = self.completed_branches.saturating_add(1);
                Some(result)
            }
            BranchOutcome::Discarded {
                vcpu,
                watchdog_firings,
            } => {
                self.add_firings(watchdog_firings);
                self.discarded_branches = self.discarded_branches.saturating_add(1);
                self.last_discarded_vcpu = Some(vcpu);
                None
            }
        }
    }

    fn add_firings(&mut self, firings: u32) {
        let firings = usize::try_from(firings).unwrap_or(usize::MAX);
        self.watchdog_firings = self.watchdog_firings.saturating_add(firings);
    }

    /// Fail closed when the round attempted branches but discarded all of them.
    pub fn ensure_admissible(&self) -> Result<(), VmError> {
        match self.last_discarded_vcpu {
            Some(vcpu) if self.completed_branches == 0 => {
                Err(VmError::HostWatchdogTimeout { vcpu })
            }
            _ => Ok(()),
        }
    }
}

/// Bootstrap state retained so a poisoned controller can be replaced.
///
/// Guest memory shares the incremental-snapshot base images, so retaining it
/// does not copy guest memory.
#[derive(Clone)]
pub struct RecoverySnapshot {
    bootstrap: SimulationSnapshot,
    memory_bases: Vec<Arc<Vec<u8>>>,
}

impl RecoverySnapshot {
    /// Retain `bootstrap`, sharing memory with `memory_bases` when present.
    ///
    /// `memory_bases` must come from `bootstrap` via
    /// `SimulationController::extract_memory_bases`, or be empty.
    pub fn new(bootstrap: &SimulationSnapshot, memory_bases: Vec<Arc<Vec<u8>>>) -> Self {
        let mut retained = bootstrap.clone();
        for ((vm_snapshot, _), base) in retained.vm_snapshots.iter_mut().zip(&memory_bases) {
            vm_snapshot.memory = SnapshotMemory::Overlay {
                base: Arc::clone(base),
                dirty_pages: BTreeMap::new(),
            };
        }
        Self {
            bootstrap: retained,
            memory_bases,
        }
    }

    /// Build a healthy controller in the post-bootstrap memory state.
    ///
    /// Guest memory equals the bootstrap base afterwards, which is the
    /// precondition for the next incremental restore. The replacement does not
    /// write determinism logs so it cannot overwrite the original controller's.
    pub fn rebuild_controller(
        &self,
        mut config: SimulationConfig,
    ) -> Result<SimulationController, VmError> {
        config.dlog_dir = None;
        let mut controller = SimulationController::new(config)?;
        let mut full = self.bootstrap.clone();
        for (vm_snapshot, _) in &mut full.vm_snapshots {
            vm_snapshot.memory = SnapshotMemory::Full(vm_snapshot.memory.materialize());
        }
        controller.restore_all(&full)?;
        controller.reset_vm_statuses();
        if !self.memory_bases.is_empty() {
            controller.set_memory_bases(self.memory_bases.clone());
        }
        Ok(controller)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coverage::CoverageBitmap;
    use crate::explorer::BranchTimings;
    use chaoscontrol_fault::oracle::OracleReport;
    use chaoscontrol_fault::schedule::FaultSchedule;

    fn result() -> BranchResult {
        BranchResult {
            coverage: CoverageBitmap::new(),
            oracle_report: OracleReport::empty(),
            schedule: FaultSchedule::new(),
            exit_counts: Vec::new(),
            halted: true,
            total_ticks: 0,
            bugs: Vec::new(),
            snapshot: None,
            schedule_variant: None,
            schedule_fingerprint: 0,
            timings: BranchTimings::default(),
        }
    }

    #[test]
    fn only_host_watchdog_is_classified_as_timeout() {
        let watchdog = ExploreError::Vm {
            source: VmError::HostWatchdogTimeout { vcpu: 2 },
        };
        assert_eq!(explore_error_watchdog_vcpu(&watchdog), Some(2));

        let internal = ExploreError::Vm {
            source: VmError::KvmInternalError { vcpu: 0 },
        };
        assert_eq!(explore_error_watchdog_vcpu(&internal), None);
        let poisoned = VmError::ControllerRoundPoisoned {
            round: 1,
            tick: 1,
            detail: "Host watchdog interrupted vCPU 0".to_string(),
        };
        assert_eq!(host_watchdog_vcpu(&poisoned), None);
        let config = ExploreError::Config {
            message: "HostWatchdogTimeout".to_string(),
        };
        assert_eq!(explore_error_watchdog_vcpu(&config), None);
    }

    #[test]
    fn retries_until_attempt_bound_then_discards() {
        for attempts in 1..MAX_BRANCH_WATCHDOG_ATTEMPTS {
            assert_eq!(
                decide_after_watchdog(attempts, true),
                WatchdogDecision::Retry
            );
        }
        assert_eq!(
            decide_after_watchdog(MAX_BRANCH_WATCHDOG_ATTEMPTS, true),
            WatchdogDecision::Discard
        );
    }

    #[test]
    fn branch_without_parent_snapshot_is_never_retried() {
        assert_eq!(decide_after_watchdog(1, false), WatchdogDecision::Discard);
    }

    #[test]
    fn retried_completion_counts_firings_without_discard() {
        let mut ledger = RoundWatchdogLedger::default();
        let admitted = ledger.admit(BranchOutcome::Completed {
            result: result(),
            watchdog_firings: 1,
        });
        assert!(admitted.is_some());
        assert_eq!(ledger.watchdog_firings, 1);
        assert_eq!(ledger.discarded_branches, 0);
        assert!(ledger.ensure_admissible().is_ok());
    }

    #[test]
    fn partial_discard_keeps_round_admissible() {
        let mut ledger = RoundWatchdogLedger::default();
        assert!(ledger.admit(BranchOutcome::completed(result())).is_some());
        let discarded = ledger.admit(BranchOutcome::Discarded {
            vcpu: 0,
            watchdog_firings: MAX_BRANCH_WATCHDOG_ATTEMPTS,
        });
        assert!(discarded.is_none());
        assert_eq!(ledger.attempted_branches, 2);
        assert_eq!(ledger.completed_branches, 1);
        assert_eq!(ledger.discarded_branches, 1);
        assert_eq!(
            ledger.watchdog_firings,
            MAX_BRANCH_WATCHDOG_ATTEMPTS as usize
        );
        assert!(ledger.ensure_admissible().is_ok());
    }

    #[test]
    fn all_discarded_round_fails_closed_with_watchdog() {
        let mut ledger = RoundWatchdogLedger::default();
        for _ in 0..2 {
            ledger.admit(BranchOutcome::Discarded {
                vcpu: 1,
                watchdog_firings: MAX_BRANCH_WATCHDOG_ATTEMPTS,
            });
        }
        assert!(matches!(
            ledger.ensure_admissible(),
            Err(VmError::HostWatchdogTimeout { vcpu: 1 })
        ));
    }

    #[test]
    fn empty_round_is_admissible() {
        assert!(RoundWatchdogLedger::default().ensure_admissible().is_ok());
    }
}
