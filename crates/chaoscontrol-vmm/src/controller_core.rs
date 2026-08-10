//! Pure multi-VM round, fault, and observation commit plans.
//!
//! The controller shell supplies observations and performs effects. This module
//! decides whether a failed operation permanently poisons later execution.

// r[impl chaoscontrol.architecture_modules.controller]
// r[impl chaoscontrol.architecture_modules.boundary]

use chaoscontrol_fault::outcomes::{
    validate_pending_fault_effect, FaultApplicationFailureDisposition,
    FaultApplicationFailureReason, FaultAttemptId, FaultOutcomeLedger, FaultPlanEffect,
    FaultTransitionError, FaultVmStatus,
};
use chaoscontrol_sim_core::CoreVmStatus;
use serde::{Deserialize, Serialize};

/// Current status of one VM in a controller round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VmStatus {
    /// VM is running normally.
    Running,
    /// VM is paused and will resume after its bounded fault effect.
    Paused,
    /// VM has crashed because a process-kill effect was applied.
    Crashed,
    /// Crashed VM will restart after this simulation tick.
    Restarting { restart_at_tick: u64 },
    /// Paused VM will resume without restore at this tick.
    Resuming { resume_at_tick: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FaultApplicationError {
    pub(crate) reason: FaultApplicationFailureReason,
    pub(crate) disposition: FaultApplicationFailureDisposition,
}

pub(crate) fn all_setup_complete(statuses: impl Iterator<Item = bool>) -> bool {
    let mut saw_vm = false;
    for status in statuses {
        saw_vm = true;
        if !status {
            return false;
        }
    }
    saw_vm
}

pub(crate) fn fault_vm_status(status: VmStatus) -> FaultVmStatus {
    match status {
        VmStatus::Running => FaultVmStatus::Running,
        VmStatus::Paused => FaultVmStatus::Paused,
        VmStatus::Crashed => FaultVmStatus::Crashed,
        VmStatus::Restarting { .. } => FaultVmStatus::Restarting,
        VmStatus::Resuming { .. } => FaultVmStatus::Resuming,
    }
}

pub(crate) fn core_vm_status(status: VmStatus) -> CoreVmStatus {
    match status {
        VmStatus::Running => CoreVmStatus::Running,
        VmStatus::Paused => CoreVmStatus::Paused,
        VmStatus::Crashed => CoreVmStatus::Crashed,
        VmStatus::Restarting { .. } => CoreVmStatus::Restarting,
        VmStatus::Resuming { .. } => CoreVmStatus::Resuming,
    }
}

pub(crate) fn checked_usize(value: u32) -> Result<usize, FaultApplicationError> {
    usize::try_from(value).map_err(|_| internal_application_error())
}

pub(crate) fn checked_usize_u64(value: u64) -> Result<usize, FaultApplicationError> {
    usize::try_from(value).map_err(|_| internal_application_error())
}

pub(crate) fn u32_targets_to_usize(values: &[u32]) -> Result<Vec<usize>, FaultApplicationError> {
    values.iter().copied().map(checked_usize).collect()
}

pub(crate) fn internal_application_error() -> FaultApplicationError {
    FaultApplicationError {
        reason: FaultApplicationFailureReason::InternalInvariant,
        disposition: FaultApplicationFailureDisposition::RolledBack,
    }
}

pub(crate) fn target_state_application_error() -> FaultApplicationError {
    FaultApplicationError {
        reason: FaultApplicationFailureReason::TargetStateChanged,
        disposition: FaultApplicationFailureDisposition::RolledBack,
    }
}

pub(crate) fn device_disappeared_application_error() -> FaultApplicationError {
    FaultApplicationError {
        reason: FaultApplicationFailureReason::DeviceDisappeared,
        disposition: FaultApplicationFailureDisposition::RolledBack,
    }
}

pub(crate) fn non_runnable_application_error() -> FaultApplicationError {
    FaultApplicationError {
        reason: FaultApplicationFailureReason::BackendRejected,
        disposition: FaultApplicationFailureDisposition::NonRunnable,
    }
}

pub(crate) fn validate_process_snapshot_effect(
    ledger: &FaultOutcomeLedger,
    target: u32,
    status: VmStatus,
    attempt_id: Option<FaultAttemptId>,
    has_pending_observation: bool,
) -> Result<(), FaultTransitionError> {
    let effect = match (status, attempt_id) {
        (VmStatus::Crashed, Some(attempt_id)) => {
            Some((attempt_id, FaultPlanEffect::ProcessKill { target }))
        }
        (VmStatus::Restarting { restart_at_tick }, Some(attempt_id)) => Some((
            attempt_id,
            FaultPlanEffect::ProcessRestart {
                target,
                restart_at_tick,
            },
        )),
        (VmStatus::Resuming { resume_at_tick }, Some(attempt_id)) => Some((
            attempt_id,
            FaultPlanEffect::ProcessPause {
                target,
                resume_at_tick,
            },
        )),
        (VmStatus::Running | VmStatus::Paused, Some(attempt_id)) if has_pending_observation => {
            let state = ledger
                .attempts
                .get(&attempt_id)
                .ok_or(FaultTransitionError::UnknownAttempt)?;
            match state.applicable_effect.as_ref() {
                Some(FaultPlanEffect::ProcessRestart {
                    target: effect_target,
                    ..
                }) if *effect_target == target => None,
                _ => return Err(FaultTransitionError::SnapshotPendingStateMismatch),
            }
        }
        (VmStatus::Restarting { .. } | VmStatus::Resuming { .. }, None)
        | (VmStatus::Running | VmStatus::Paused, Some(_)) => {
            return Err(FaultTransitionError::SnapshotPendingStateMismatch);
        }
        (VmStatus::Running | VmStatus::Paused | VmStatus::Crashed, None) => None,
    };
    if let Some((attempt_id, effect)) = effect {
        validate_pending_fault_effect(ledger, attempt_id, &effect)?;
    }
    Ok(())
}

/// Facts available when one controller operation finishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompletionFacts {
    pub(crate) mutation_started: bool,
    pub(crate) operation_failed: bool,
    pub(crate) poison_already_latched: bool,
}

/// Pure completion decision for a controller operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompletionPlan {
    pub(crate) latch_first_failure: bool,
    pub(crate) return_original_result: bool,
}

/// Preserve the first failed mutation and never replace its diagnostic.
pub(crate) fn plan_completion(facts: CompletionFacts) -> CompletionPlan {
    CompletionPlan {
        latch_first_failure: facts.mutation_started
            && facts.operation_failed
            && !facts.poison_already_latched,
        return_original_result: true,
    }
}

/// Return the next bounded operation number without integer wraparound.
pub(crate) fn next_operation(current: u64) -> u64 {
    current.saturating_add(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_mutation_latches_the_first_failure() {
        let plan = plan_completion(CompletionFacts {
            mutation_started: true,
            operation_failed: true,
            poison_already_latched: false,
        });
        assert!(plan.latch_first_failure);
        assert!(plan.return_original_result);
    }

    #[test]
    fn preflight_failure_does_not_poison_future_rounds() {
        let plan = plan_completion(CompletionFacts {
            mutation_started: false,
            operation_failed: true,
            poison_already_latched: false,
        });
        assert!(!plan.latch_first_failure);
        assert!(plan.return_original_result);
    }

    #[test]
    fn later_failure_cannot_replace_the_first_failure() {
        let plan = plan_completion(CompletionFacts {
            mutation_started: true,
            operation_failed: true,
            poison_already_latched: true,
        });
        assert!(!plan.latch_first_failure);
    }

    #[test]
    fn operation_numbers_saturate_instead_of_wrapping() {
        assert_eq!(next_operation(0), 1);
        assert_eq!(next_operation(u64::MAX), u64::MAX);
    }
}
