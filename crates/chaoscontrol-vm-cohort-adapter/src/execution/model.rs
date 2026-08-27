use vm_cohort_core::CohortState;
use vm_cohort_kvm::KvmCohortRuntime;

use crate::ChaosCohortOutcome;

/// Consumer-owned failure class for one cohort execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChaosExecutionFailureKind {
    /// VM Cohort could not execute one admitted mechanism effect.
    KvmEffect,
    /// ChaosControl could not apply exact snapshot state.
    SnapshotRestore,
}

/// Active cohort with the live VM Cohort runtime retained by the caller.
pub struct ActiveChaosCohort {
    /// Bounded active-state and authority observation.
    pub outcome: ChaosCohortOutcome,
    /// Live KVM descriptors and clone-private state.
    pub runtime: KvmCohortRuntime,
}

/// Failed cohort with exact cleanup state and optional live uncertain resources.
pub struct FailedChaosCohort {
    /// Final or cleanup-uncertain shared lifecycle state.
    pub state: CohortState,
    /// Consumer failure class.
    pub kind: ChaosExecutionFailureKind,
    /// Bounded mechanism or snapshot diagnostic.
    pub detail: String,
    /// Whether cleanup has an unknown outcome.
    pub cleanup_uncertain: bool,
    /// Retained runtime when an operator must resolve uncertain cleanup.
    pub runtime: Option<KvmCohortRuntime>,
    /// Failure evidence did not grant fault authority.
    pub fault_authority_granted: bool,
    /// Failure evidence did not grant replay authority.
    pub replay_authority_granted: bool,
    /// Failure evidence did not grant release authority.
    pub release_authority_granted: bool,
}

/// Closed result of one admitted cohort execution.
pub enum ChaosCohortExecution {
    /// Every clone activated after exact restore.
    Active(ActiveChaosCohort),
    /// Creation or restore failed and cleanup state is explicit.
    Failed(FailedChaosCohort),
}
