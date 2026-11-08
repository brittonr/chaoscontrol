//! Deterministic serialized vCPU scheduler.
//!
//! Multi-vCPU guests run one vCPU at a time. Typed guest progress,
//! replay-stable runnable state, seeded policy state, and declared boundaries are
//! owned by the pure [`core`] transition. The KVM shell reserves evidence
//! capacity before guest execution and applies a planned action only after the
//! transition record is validated and committed.

use serde::{Deserialize, Serialize};

/// Pure deterministic schedule transitions and evidence validation.
pub mod core;

use core::{
    reconfigure_policy, transition, validate_state, ExactStepState, PlannedScheduleTransition,
    ProgressBudget, ProgressMode, ScheduleError, ScheduleEvent, ScheduleJournal,
    ScheduleReservation, ScheduleState, ScheduleStateId, ScheduleTrace, ScheduleTransitionRecord,
    DEFAULT_SCHEDULE_JOURNAL_LIMIT,
};

/// Scheduler snapshot schema version.
pub const SCHEDULER_SNAPSHOT_SCHEMA_VERSION: u16 = 2;
/// Default instruction quantum for deterministic SMP.
pub const DEFAULT_SMP_INSTRUCTION_QUANTUM: u64 = 500_000;
/// Default exact single-step margin for PMU acceleration.
pub const DEFAULT_PMU_EXACT_STEP_MARGIN: u64 = 50;

/// Scheduling strategy for multi-vCPU VMs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SchedulingStrategy {
    /// Fixed round-robin using the declared progress budget.
    RoundRobin,
    /// Each turn draws a seeded budget in `[min_budget, max_budget)`.
    Randomized {
        /// Inclusive minimum, with the same unit/profile as configuration.
        min_budget: ProgressBudget,
        /// Exclusive maximum, with the same unit/profile as configuration.
        max_budget: ProgressBudget,
    },
}

/// Configuration for the vCPU scheduler.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Number of vCPUs to schedule.
    pub num_vcpus: usize,
    /// Initial and round-robin turn budget, with explicit units.
    pub progress_budget: ProgressBudget,
    /// Scheduling strategy.
    pub strategy: SchedulingStrategy,
    /// Seed for deterministic policy choices.
    pub seed: u64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            num_vcpus: 1,
            progress_budget: ProgressBudget::Instructions(DEFAULT_SMP_INSTRUCTION_QUANTUM),
            strategy: SchedulingStrategy::RoundRobin,
            seed: 0,
        }
    }
}

/// Deterministic scheduler shell around the pure transition core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VcpuScheduler {
    admitted_num_vcpus: usize,
    admitted_progress_mode: ProgressMode,
    journal: ScheduleJournal,
}

impl VcpuScheduler {
    /// Create a scheduler after bounded configuration and initial-state checks.
    pub fn try_new(
        config: &SchedulerConfig,
        progress_mode: ProgressMode,
        runnable_vcpus: Vec<bool>,
    ) -> Result<Self, ScheduleError> {
        Self::try_new_with_journal_limit(
            config,
            progress_mode,
            runnable_vcpus,
            DEFAULT_SCHEDULE_JOURNAL_LIMIT,
        )
    }

    /// Create a scheduler with an explicit bounded evidence journal.
    pub fn try_new_with_journal_limit(
        config: &SchedulerConfig,
        progress_mode: ProgressMode,
        runnable_vcpus: Vec<bool>,
        journal_limit: usize,
    ) -> Result<Self, ScheduleError> {
        let state = ScheduleState::new_with_runnable(config, progress_mode, runnable_vcpus)?;
        let journal = ScheduleJournal::new(state, journal_limit)?;
        Ok(Self {
            admitted_num_vcpus: config.num_vcpus,
            admitted_progress_mode: progress_mode,
            journal,
        })
    }

    /// Current complete deterministic schedule state.
    pub fn state(&self) -> &ScheduleState {
        self.journal.state()
    }

    /// Current BLAKE3 schedule-state identity.
    pub fn state_id(&self) -> ScheduleStateId {
        self.state().identity()
    }

    /// Currently selected vCPU.
    pub fn active(&self) -> usize {
        self.state().active_vcpu
    }

    /// Number of admitted vCPUs.
    pub fn num_vcpus(&self) -> usize {
        self.admitted_num_vcpus
    }

    /// Declared deterministic progress mode.
    pub fn progress_mode(&self) -> ProgressMode {
        self.admitted_progress_mode
    }

    /// Typed progress remaining before the active vCPU's declared turn boundary.
    pub fn remaining_budget(&self) -> ProgressBudget {
        self.state().turn_boundary().with_count(
            self.state().turn_boundary().count() - self.state().progress_counts()[self.active()],
        )
    }

    /// Instruction-only exact-step state; boundary mode has no remainder.
    pub fn exact_step(&self) -> Result<ExactStepState, ScheduleError> {
        self.state().exact_step()
    }

    /// True when guest execution owns a journal reservation.
    pub fn reservation_outstanding(&self) -> bool {
        self.journal.reservation_outstanding()
    }

    /// Reserve evidence storage before `KVM_RUN` or guest-debug execution.
    pub fn reserve_transition(&mut self) -> Result<ScheduleReservation, ScheduleError> {
        self.journal.reserve()
    }

    /// Release a preflight slot after a shell failure with no guest progress.
    pub fn release_transition(
        &mut self,
        reservation: ScheduleReservation,
    ) -> Result<(), ScheduleError> {
        self.journal.release(reservation)
    }

    /// Compute a transition without changing scheduler or shell state.
    pub fn plan(&self, event: &ScheduleEvent) -> Result<PlannedScheduleTransition, ScheduleError> {
        transition(self.state(), event)
    }

    /// Commit a validated transition into its preflighted evidence slot.
    pub fn commit(
        &mut self,
        reservation: ScheduleReservation,
        planned: PlannedScheduleTransition,
    ) -> Result<ScheduleTransitionRecord, ScheduleError> {
        if planned.record.pre_state_id != self.journal.state().identity() {
            return Err(ScheduleError::StaleState {
                expected: planned.record.pre_state_id,
                actual: self.journal.state().identity(),
            });
        }
        let record = planned.record;
        self.journal.commit(reservation, record.clone())?;
        Ok(record)
    }

    /// Drain committed evidence as one independently verifiable trace.
    pub fn drain_trace(&mut self) -> Result<ScheduleTrace, ScheduleError> {
        self.journal.drain()
    }

    /// Return a compact legacy branch fingerprint derived from BLAKE3 state.
    pub fn fingerprint(&self) -> u64 {
        const FINGERPRINT_BYTES: usize = 8;
        let identity = self.state_id();
        let mut prefix = [0u8; FINGERPRINT_BYTES];
        prefix.copy_from_slice(&identity.0[..FINGERPRINT_BYTES]);
        u64::from_le_bytes(prefix)
    }

    /// Produce an exact serializable scheduler snapshot.
    // r[impl chaoscontrol.deterministic_smp.snapshot_state]
    pub fn snapshot(&self) -> SchedulerSnapshot {
        SchedulerSnapshot {
            schema_version: SCHEDULER_SNAPSHOT_SCHEMA_VERSION,
            state: self.state().clone(),
            state_id: self.state_id(),
        }
    }

    /// Validate an untrusted snapshot against this VM's admitted profile.
    pub fn validate_snapshot(&self, snapshot: &SchedulerSnapshot) -> Result<(), ScheduleError> {
        snapshot.validate()?;
        if snapshot.state.num_vcpus != self.admitted_num_vcpus {
            return Err(ScheduleError::InvalidConfiguration {
                reason: "snapshot vCPU count differs from the VM profile",
            });
        }
        if snapshot.state.progress_mode != self.admitted_progress_mode {
            return Err(ScheduleError::InvalidConfiguration {
                reason: "snapshot progress mode differs from the VM profile",
            });
        }
        if !snapshot
            .state
            .progress_budget()
            .same_unit(self.state().progress_budget())
        {
            return Err(ScheduleError::ProgressUnitMismatch);
        }
        Ok(())
    }

    /// Restore exact state only after complete validation succeeds.
    pub fn restore(&mut self, snapshot: &SchedulerSnapshot) -> Result<(), ScheduleError> {
        self.validate_snapshot(snapshot)?;
        self.journal.reset(snapshot.state.clone())
    }

    /// Check an externally recorded variant against the policy actually executing.
    /// Unspecified overrides preserve the restored policy, rather than imposing defaults.
    pub fn matches_variant(&self, variant: &ScheduleVariant) -> bool {
        let state = self.state();
        state.rng_seed == core::policy_rng_seed(variant.scheduler_seed)
            && variant
                .progress_budget_override
                .is_none_or(|budget| state.progress_budget() == budget)
            && variant
                .strategy_override
                .is_none_or(|strategy| state.strategy == strategy)
    }

    /// Apply an explicit seeded policy variant through the pure core.
    pub fn apply_variant(&mut self, variant: &ScheduleVariant) -> Result<(), ScheduleError> {
        let state = self.state();
        let config = SchedulerConfig {
            num_vcpus: self.admitted_num_vcpus,
            progress_budget: variant
                .progress_budget_override
                .unwrap_or(state.progress_budget()),
            strategy: variant.strategy_override.unwrap_or(state.strategy),
            seed: variant.scheduler_seed,
        };
        let configured = reconfigure_policy(state, &config)?;
        self.journal.reset(configured)
    }
}

/// Exact serialized scheduler state and identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchedulerSnapshot {
    /// Snapshot schema version.
    pub schema_version: u16,
    /// Complete deterministic schedule state.
    pub state: ScheduleState,
    /// BLAKE3 identity of `state`.
    pub state_id: ScheduleStateId,
}

impl SchedulerSnapshot {
    /// Validate an untrusted snapshot without changing live state.
    pub fn validate(&self) -> Result<(), ScheduleError> {
        if self.schema_version != SCHEDULER_SNAPSHOT_SCHEMA_VERSION {
            return Err(ScheduleError::UnsupportedSchema {
                found: self.schema_version,
            });
        }
        validate_state(&self.state)?;
        let actual = self.state.identity();
        if actual != self.state_id {
            return Err(ScheduleError::IdentityMismatch {
                field: "scheduler snapshot state_id",
                expected: self.state_id,
                actual,
            });
        }
        Ok(())
    }
}

/// Per-branch scheduling overrides for schedule diversity exploration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleVariant {
    /// Scheduler RNG seed for this branch.
    pub scheduler_seed: u64,
    /// Override the scheduling strategy.
    pub strategy_override: Option<SchedulingStrategy>,
    /// Override the typed progress budget; units/profile cannot change.
    pub progress_budget_override: Option<ProgressBudget>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::core::{ProgressSource, ScheduleAction};

    const VCPU_COUNT: usize = 2;
    const QUANTUM: u64 = 3;
    const TEST_SEED: u64 = 42;
    const ALTERNATE_TEST_SEED: u64 = 43;

    fn config() -> SchedulerConfig {
        SchedulerConfig {
            num_vcpus: VCPU_COUNT,
            progress_budget: ProgressBudget::Instructions(QUANTUM),
            strategy: SchedulingStrategy::RoundRobin,
            seed: TEST_SEED,
        }
    }

    fn scheduler() -> VcpuScheduler {
        VcpuScheduler::try_new(
            &config(),
            ProgressMode::ExactSingleStep,
            vec![true; VCPU_COUNT],
        )
        .unwrap()
    }

    fn progress_event(scheduler: &VcpuScheduler) -> ScheduleEvent {
        let state = scheduler.state();
        ScheduleEvent::InstructionProgress {
            expected_state_id: state.identity(),
            vcpu: state.active_vcpu,
            observed_progress: state.progress_counts()[state.active_vcpu] + 1,
            runnable_changes: Vec::new(),
            source: ProgressSource::ExactSingleStep,
        }
    }

    fn commit_progress(scheduler: &mut VcpuScheduler) -> ScheduleAction {
        let reservation = scheduler.reserve_transition().unwrap();
        let planned = scheduler.plan(&progress_event(scheduler)).unwrap();
        let action = planned.record.action.clone();
        scheduler.commit(reservation, planned).unwrap();
        action
    }

    #[test]
    fn wrapper_switches_only_after_exact_quantum() {
        let mut scheduler = scheduler();
        assert_eq!(commit_progress(&mut scheduler), ScheduleAction::Continue);
        assert_eq!(commit_progress(&mut scheduler), ScheduleAction::Continue);
        assert!(matches!(
            commit_progress(&mut scheduler),
            ScheduleAction::Switch {
                from_vcpu: 0,
                to_vcpu: 1,
                ..
            }
        ));
        assert_eq!(scheduler.active(), 1);
    }

    #[test]
    // r[verify chaoscontrol.deterministic_smp.snapshot_state]
    // r[verify chaoscontrol.deterministic_smp.validation.snapshot]
    fn snapshot_restore_preserves_identity_and_future() {
        let mut original = scheduler();
        commit_progress(&mut original);
        let snapshot = original.snapshot();
        let expected_identity = original.state_id();

        let mut restored = scheduler();
        restored.restore(&snapshot).unwrap();
        assert_eq!(restored.state_id(), expected_identity);
        assert_eq!(
            commit_progress(&mut original),
            commit_progress(&mut restored)
        );
        assert_eq!(original.state_id(), restored.state_id());
    }

    #[test]
    // r[verify chaoscontrol.deterministic_smp.snapshot_state]
    // r[verify chaoscontrol.deterministic_smp.validation.snapshot]
    fn snapshot_restore_preserves_pmu_exact_remainder_future() {
        let progress_mode = ProgressMode::PmuAccelerated {
            exact_step_margin: 1,
        };
        let mut original =
            VcpuScheduler::try_new(&config(), progress_mode, vec![true; VCPU_COUNT]).unwrap();
        let reservation = original.reserve_transition().unwrap();
        let planned = core::plan_execution_observation(
            original.state(),
            core::ExecutionProgressObservation::PmuInterrupt {
                vcpu: 0,
                counter_base_progress: 0,
                counter_value: QUANTUM - 1,
            },
        )
        .unwrap()
        .unwrap();
        original.commit(reservation, planned).unwrap();
        assert_eq!(
            original.exact_step().unwrap(),
            ExactStepState::Active { remaining: 1 }
        );
        let snapshot = original.snapshot();

        let mut restored =
            VcpuScheduler::try_new(&config(), progress_mode, vec![true; VCPU_COUNT]).unwrap();
        restored.restore(&snapshot).unwrap();
        assert_eq!(
            commit_progress(&mut original),
            commit_progress(&mut restored)
        );
        assert_eq!(original.state_id(), restored.state_id());
    }

    #[test]
    fn forged_snapshot_is_rejected_without_mutation() {
        let mut scheduler = scheduler();
        commit_progress(&mut scheduler);
        let before = scheduler.clone();
        let mut forged = scheduler.snapshot();
        forged
            .state
            .progress
            .set_turn_boundary(forged.state.turn_boundary().count() + 1);

        assert!(matches!(
            scheduler.restore(&forged),
            Err(ScheduleError::IdentityMismatch { .. })
        ));
        assert_eq!(scheduler, before);
    }

    #[test]
    fn policy_variant_is_seeded_and_preserves_active_vcpu() {
        let mut first = scheduler();
        let mut second = scheduler();
        let baseline_fingerprint = first.fingerprint();
        let variant = ScheduleVariant {
            scheduler_seed: TEST_SEED,
            strategy_override: Some(SchedulingStrategy::Randomized {
                min_budget: ProgressBudget::Instructions(1),
                max_budget: ProgressBudget::Instructions(QUANTUM + 1),
            }),
            progress_budget_override: None,
        };
        first.apply_variant(&variant).unwrap();
        second.apply_variant(&variant).unwrap();
        assert_eq!(first.state_id(), second.state_id());
        assert_ne!(first.fingerprint(), baseline_fingerprint);
        assert_eq!(first.active(), 0);

        let mut drifted = scheduler();
        drifted
            .apply_variant(&ScheduleVariant {
                scheduler_seed: ALTERNATE_TEST_SEED,
                ..variant
            })
            .unwrap();
        assert_ne!(first.fingerprint(), drifted.fingerprint());
    }

    #[test]
    fn explicit_journal_bound_is_enforced_and_preserved_on_restore() {
        let mut bounded = VcpuScheduler::try_new_with_journal_limit(
            &config(),
            ProgressMode::ExactSingleStep,
            vec![true; VCPU_COUNT],
            1,
        )
        .unwrap();
        commit_progress(&mut bounded);
        assert!(matches!(
            bounded.reserve_transition(),
            Err(ScheduleError::JournalCapacityExceeded { limit: 1 })
        ));

        let snapshot = bounded.snapshot();
        bounded.restore(&snapshot).unwrap();
        commit_progress(&mut bounded);
        assert!(matches!(
            bounded.reserve_transition(),
            Err(ScheduleError::JournalCapacityExceeded { limit: 1 })
        ));
    }

    #[test]
    fn excessive_explicit_journal_bound_is_rejected() {
        assert!(matches!(
            VcpuScheduler::try_new_with_journal_limit(
                &config(),
                ProgressMode::ExactSingleStep,
                vec![true; VCPU_COUNT],
                DEFAULT_SCHEDULE_JOURNAL_LIMIT + 1,
            ),
            Err(ScheduleError::JournalLimitExceeded { .. })
        ));
    }

    #[test]
    fn mismatched_progress_profile_snapshot_is_rejected() {
        let exact = scheduler();
        let pmu = VcpuScheduler::try_new(
            &config(),
            ProgressMode::PmuAccelerated {
                exact_step_margin: 1,
            },
            vec![true; VCPU_COUNT],
        )
        .unwrap();
        assert!(matches!(
            exact.validate_snapshot(&pmu.snapshot()),
            Err(ScheduleError::InvalidConfiguration { .. })
        ));
    }

    #[test]
    fn live_variant_check_detects_restored_seed_strategy_and_quantum_drift() {
        let mut running = scheduler();
        let original = running.snapshot();
        let variant = ScheduleVariant {
            scheduler_seed: 173,
            strategy_override: Some(SchedulingStrategy::Randomized {
                min_budget: ProgressBudget::Instructions(2),
                max_budget: ProgressBudget::Instructions(7),
            }),
            progress_budget_override: Some(ProgressBudget::Instructions(8)),
        };
        running.apply_variant(&variant).unwrap();
        assert!(running.matches_variant(&variant));
        assert!(!running.matches_variant(&ScheduleVariant {
            scheduler_seed: 174,
            ..variant.clone()
        }));
        assert!(!running.matches_variant(&ScheduleVariant {
            strategy_override: Some(SchedulingStrategy::RoundRobin),
            ..variant.clone()
        }));
        assert!(!running.matches_variant(&ScheduleVariant {
            progress_budget_override: Some(ProgressBudget::Instructions(9)),
            ..variant.clone()
        }));
        assert!(running.matches_variant(&ScheduleVariant {
            strategy_override: None,
            progress_budget_override: None,
            ..variant.clone()
        }));
        running.restore(&original).unwrap();
        assert!(!running.matches_variant(&variant));
    }

    #[test]
    fn coarse_snapshot_preserves_prefix_future_and_rejects_unit_drift() {
        use crate::scheduler::core::{
            BoundaryProfileId, ExecutionProgressObservation, KvmBoundaryKind,
        };
        let budget = ProgressBudget::KvmBoundaries {
            profile_id: BoundaryProfileId::KvmDebugExitBoundaryV1,
            count: QUANTUM,
        };
        let config = SchedulerConfig {
            progress_budget: budget,
            ..config()
        };
        let mut original = VcpuScheduler::try_new(
            &config,
            ProgressMode::KvmDebugBoundary,
            vec![true; VCPU_COUNT],
        )
        .unwrap();
        assert_eq!(original.remaining_budget(), budget);
        assert_eq!(
            original.exact_step(),
            Err(ScheduleError::ExactStepUnavailable)
        );
        let observation = ExecutionProgressObservation::KvmBoundary {
            vcpu: 0,
            profile_id: BoundaryProfileId::KvmDebugExitBoundaryV1,
            boundary: KvmBoundaryKind::DebugSingleStep,
        };
        let planned = core::plan_execution_observation(original.state(), observation)
            .unwrap()
            .unwrap();
        let reservation = original.reserve_transition().unwrap();
        original.commit(reservation, planned).unwrap();
        assert_eq!(original.remaining_budget(), budget.with_count(QUANTUM - 1));
        let bytes = serde_json::to_vec(&original.snapshot()).unwrap();
        let decoded: SchedulerSnapshot = serde_json::from_slice(&bytes).unwrap();
        let mut restored = VcpuScheduler::try_new(
            &config,
            ProgressMode::KvmDebugBoundary,
            vec![true; VCPU_COUNT],
        )
        .unwrap();
        restored.restore(&decoded).unwrap();
        assert_eq!(restored.state_id(), original.state_id());
        assert_eq!(
            core::plan_execution_observation(original.state(), observation).unwrap(),
            core::plan_execution_observation(restored.state(), observation).unwrap()
        );
        let before = restored.clone();
        assert!(restored.restore(&scheduler().snapshot()).is_err());
        assert_eq!(restored, before);
        let mixed = ScheduleVariant {
            scheduler_seed: TEST_SEED,
            progress_budget_override: Some(ProgressBudget::Instructions(QUANTUM)),
            strategy_override: None,
        };
        assert_eq!(
            restored.apply_variant(&mixed),
            Err(ScheduleError::ProgressUnitMismatch)
        );
        assert_eq!(restored, before);
        let mut wrong_schema = decoded;
        wrong_schema.schema_version = 1;
        assert_eq!(
            restored.restore(&wrong_schema),
            Err(ScheduleError::UnsupportedSchema { found: 1 })
        );
        assert_eq!(restored, before);
    }
}
