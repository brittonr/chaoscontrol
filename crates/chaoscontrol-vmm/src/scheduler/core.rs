//! Pure deterministic SMP schedule transitions.
//!
//! This module has no KVM, signal, clock, thread, or filesystem access.
//! The VMM shell supplies replay-stable guest progress and runnable-state
//! changes. The core validates one event and returns a complete candidate state
//! plus compact evidence. The shell must reserve evidence capacity before it
//! runs a guest, then commit the validated evidence before it applies the
//! selected-vCPU or guest-debug effects.

use super::{SchedulerConfig, SchedulingStrategy};
use rand::RngCore;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use serde::de::{self, DeserializeOwned, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

/// Canonical schedule-state schema version.
pub const SCHEDULE_STATE_SCHEMA_VERSION: u16 = 1;
/// Maximum vCPUs admitted by deterministic schedule state and evidence.
pub const MAX_SCHEDULE_VCPUS: usize = 256;
/// Default maximum records in one in-memory schedule journal or trace.
pub const DEFAULT_SCHEDULE_JOURNAL_LIMIT: usize = 65_536;

const SCHEDULE_STATE_DOMAIN: &[u8] = b"chaoscontrol.schedule-state.v1";
const SCHEDULER_SEED_DOMAIN: u64 = 0x5343_4845_4430;

/// BLAKE3 identity of one canonical [`ScheduleState`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ScheduleStateId(pub [u8; 32]);

impl fmt::Debug for ScheduleStateId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ScheduleStateId(")?;
        for byte in &self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        write!(formatter, ")")
    }
}

/// Declared source of deterministic guest-instruction progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProgressMode {
    /// Portable correctness baseline: KVM exits after each guest instruction.
    ExactSingleStep,
    /// PMU overflow approaches the boundary, then exact single-step finishes it.
    PmuAccelerated {
        /// Maximum exact single-step remainder after PMU progress.
        exact_step_margin: u64,
    },
    /// Legacy wall-clock mode. It exists only so old artifacts fail closed.
    LegacyWallClock,
}

/// Capabilities established by the imperative KVM/perf shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgressCapabilities {
    /// KVM guest-debug single-step can be enabled.
    pub exact_single_step: bool,
    /// The PMU counts guest instructions while excluding host instructions.
    pub guest_instruction_pmu: bool,
    /// PMU overflow can interrupt `KVM_RUN`.
    pub pmu_overflow: bool,
}

/// Exact-step state at the current quantum boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExactStepState {
    /// The selected progress source can run normally.
    Inactive,
    /// PMU progress stopped before the boundary. Single-step owns the remainder.
    Active {
        /// Instructions that remain before the exact boundary.
        remaining: u64,
    },
}

/// Complete deterministic scheduling state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleState {
    /// State schema version.
    pub schema_version: u16,
    /// Number of serialized vCPUs.
    pub num_vcpus: usize,
    /// Currently selected vCPU.
    pub active_vcpu: usize,
    /// Replay-stable KVM runnable observation for each vCPU.
    #[serde(deserialize_with = "deserialize_bounded_bools")]
    pub runnable_vcpus: Vec<bool>,
    /// Cumulative retired guest instructions for each vCPU.
    #[serde(deserialize_with = "deserialize_bounded_u64s")]
    pub instruction_progress: Vec<u64>,
    /// Cumulative instruction boundary for the active vCPU's turn.
    pub quantum_boundary: u64,
    /// Fixed initial quantum and round-robin quantum.
    pub quantum: u64,
    /// Deterministic scheduling policy.
    pub strategy: SchedulingStrategy,
    /// Seeded policy RNG key.
    pub rng_seed: [u8; 32],
    /// Seeded policy RNG word position.
    pub rng_word_pos: u128,
    /// Declared deterministic progress mode.
    pub progress_mode: ProgressMode,
    /// Pending exact-step remainder.
    pub exact_step: ExactStepState,
    /// Number of accepted deterministic progress transitions.
    pub sequence: u64,
    /// True after all vCPUs become non-runnable.
    pub halted: bool,
}

/// Deterministic progress source attached to an observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProgressSource {
    /// One exact guest instruction completed under KVM single-step.
    ExactSingleStep,
    /// A guest-only PMU supplied cumulative progress.
    GuestInstructionPmu,
}

/// One canonical change to the runnable set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnableChange {
    /// vCPU whose runnable state changed.
    pub vcpu: usize,
    /// New replay-stable runnable state.
    pub runnable: bool,
}

/// Host-owned input that must not have deterministic schedule authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostEventKind {
    /// `VcpuExit::Intr` or an `EINTR` error.
    SignalInterrupt,
    /// A wall-clock watchdog expired.
    WatchdogExpired,
    /// A host worker or thread became runnable.
    ThreadWake,
}

/// Typed input to the pure schedule transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScheduleEvent {
    /// Replay-stable cumulative guest-instruction progress.
    GuestProgress {
        /// Identity the producer observed before it created this event.
        expected_state_id: ScheduleStateId,
        /// vCPU that retired the instructions.
        vcpu: usize,
        /// Cumulative retired instructions for `vcpu`.
        observed_progress: u64,
        /// Sorted changes from the prior runnable set.
        #[serde(deserialize_with = "deserialize_bounded_runnable_changes")]
        runnable_changes: Vec<RunnableChange>,
        /// Exact deterministic source for `observed_progress`.
        source: ProgressSource,
    },
    /// Host-owned event. The transition always rejects this input.
    HostEvent {
        /// Identity the producer observed before it created this event.
        expected_state_id: ScheduleStateId,
        /// Host event classification.
        kind: HostEventKind,
    },
}

impl ScheduleEvent {
    fn expected_state_id(&self) -> ScheduleStateId {
        match self {
            Self::GuestProgress {
                expected_state_id, ..
            }
            | Self::HostEvent {
                expected_state_id, ..
            } => *expected_state_id,
        }
    }
}

/// Reason for a deterministic vCPU selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SwitchReason {
    /// The selected vCPU reached its exact instruction boundary.
    QuantumBoundary,
    /// A replay-stable guest/KVM state reported the selected vCPU blocked.
    ActiveVcpuBlocked,
}

/// Auditable action selected by the pure transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScheduleAction {
    /// Continue the selected vCPU with the declared progress source.
    Continue,
    /// Enable exact single-step for the stated remainder.
    EnterExactStep {
        /// Instructions that remain before the exact boundary.
        remaining: u64,
    },
    /// Select another runnable vCPU from deterministic state.
    Switch {
        /// Previously selected vCPU.
        from_vcpu: usize,
        /// Newly selected vCPU.
        to_vcpu: usize,
        /// Cumulative progress observed for `from_vcpu`.
        from_progress: u64,
        /// Exact boundary assigned to the prior turn.
        declared_boundary: u64,
        /// Why the turn ended.
        reason: SwitchReason,
        /// Cumulative boundary assigned to `to_vcpu`.
        next_boundary: u64,
    },
    /// Stop because no vCPU is runnable.
    Halt,
}

/// Compact canonical evidence for one accepted transition.
///
/// The containing [`ScheduleTrace`] owns the initial state. Each record owns the
/// event, action, and pre/post identities needed to recompute the next state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleTransitionRecord {
    /// BLAKE3 identity before the event.
    pub pre_state_id: ScheduleStateId,
    /// Accepted deterministic event.
    pub event: ScheduleEvent,
    /// Selected action.
    pub action: ScheduleAction,
    /// BLAKE3 identity after the event.
    pub post_state_id: ScheduleStateId,
}

/// Bounded, independently verifiable transition chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleTrace {
    /// Full state before the first record.
    pub initial_state: ScheduleState,
    /// BLAKE3 identity of `initial_state`.
    pub initial_state_id: ScheduleStateId,
    /// Compact transition records, bounded during deserialization.
    #[serde(deserialize_with = "deserialize_bounded_records")]
    pub records: Vec<ScheduleTransitionRecord>,
}

/// Pure transition result. The shell can validate and admit its record first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedScheduleTransition {
    /// Candidate next state.
    pub next_state: ScheduleState,
    /// Canonical evidence for the candidate transition.
    pub record: ScheduleTransitionRecord,
}

/// Opaque reservation for one preflighted journal slot.
#[derive(Debug, PartialEq, Eq)]
pub struct ScheduleReservation {
    id: u64,
}

/// Deterministic schedule validation or transition error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleError {
    /// The state schema is not supported.
    UnsupportedSchema { found: u16 },
    /// A legacy wall-clock progress mode was supplied.
    UnsupportedProgressMode,
    /// A required execution capability is absent.
    MissingCapability { capability: &'static str },
    /// Scheduler configuration is invalid.
    InvalidConfiguration { reason: &'static str },
    /// An input exceeded the vCPU allocation bound.
    TooManyVcpus { found: usize, limit: usize },
    /// State vector lengths do not match the vCPU count.
    StateLengthMismatch,
    /// A vCPU index is outside the state bounds.
    InvalidVcpu { vcpu: usize, num_vcpus: usize },
    /// Runnable-set changes are not sorted and unique.
    NonCanonicalRunnableChanges,
    /// The active vCPU is not runnable in a live state.
    ActiveVcpuNotRunnable { vcpu: usize },
    /// An event was built against a stale or forged state identity.
    StaleState {
        expected: ScheduleStateId,
        actual: ScheduleStateId,
    },
    /// Progress did not advance.
    StaleProgress { previous: u64, observed: u64 },
    /// Exact progress skipped one or more instructions.
    ImpossibleExactProgress { expected: u64, observed: u64 },
    /// Progress passed the declared exact boundary.
    ProgressOvershoot { boundary: u64, observed: u64 },
    /// The event source does not match the declared mode and exact-step state.
    ProgressSourceMismatch,
    /// A host-owned input attempted to enter the deterministic core.
    HostOwnedInput { kind: HostEventKind },
    /// A checked deterministic counter overflowed.
    CounterOverflow { counter: &'static str },
    /// A serialized identity does not match its state.
    IdentityMismatch {
        field: &'static str,
        expected: ScheduleStateId,
        actual: ScheduleStateId,
    },
    /// A transition record does not match a recomputed transition.
    TransitionMismatch { field: &'static str },
    /// Adjacent evidence records do not form one state chain.
    TraceDiscontinuity { index: usize },
    /// A bounded evidence journal has no remaining capacity.
    JournalCapacityExceeded { limit: usize },
    /// The journal could not reserve memory before guest execution.
    JournalAllocationFailed,
    /// A journal already has an outstanding preflight reservation.
    ReservationOutstanding,
    /// A reservation does not match the journal's outstanding reservation.
    InvalidReservation,
}

impl fmt::Display for ScheduleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ScheduleError {}

impl ScheduleState {
    /// Build validated initial state from scheduler configuration.
    pub fn new(
        config: &SchedulerConfig,
        progress_mode: ProgressMode,
    ) -> Result<Self, ScheduleError> {
        validate_configuration(config, progress_mode)?;
        let num_vcpus = config.num_vcpus;
        let mut rng_seed = [0u8; 32];
        let derived_seed = config.seed.wrapping_add(SCHEDULER_SEED_DOMAIN);
        let seed_bytes = derived_seed.to_le_bytes();
        rng_seed[..seed_bytes.len()].copy_from_slice(&seed_bytes);

        let state = Self {
            schema_version: SCHEDULE_STATE_SCHEMA_VERSION,
            num_vcpus,
            active_vcpu: 0,
            runnable_vcpus: vec![true; num_vcpus],
            instruction_progress: vec![0; num_vcpus],
            quantum_boundary: config.quantum,
            quantum: config.quantum,
            strategy: config.strategy,
            rng_seed,
            rng_word_pos: 0,
            progress_mode,
            exact_step: ExactStepState::Inactive,
            sequence: 0,
            halted: false,
        };
        validate_state(&state)?;
        Ok(state)
    }

    /// Return the canonical BLAKE3 identity of this state.
    pub fn identity(&self) -> ScheduleStateId {
        schedule_state_identity(self)
    }
}

/// Validate a requested progress mode against shell-probed capabilities.
pub fn validate_progress_capabilities(
    mode: ProgressMode,
    capabilities: ProgressCapabilities,
) -> Result<(), ScheduleError> {
    match mode {
        ProgressMode::ExactSingleStep => {
            if !capabilities.exact_single_step {
                return Err(ScheduleError::MissingCapability {
                    capability: "KVM exact single-step",
                });
            }
        }
        ProgressMode::PmuAccelerated { .. } => {
            if !capabilities.exact_single_step {
                return Err(ScheduleError::MissingCapability {
                    capability: "KVM exact single-step",
                });
            }
            if !capabilities.guest_instruction_pmu {
                return Err(ScheduleError::MissingCapability {
                    capability: "guest-only instruction PMU",
                });
            }
            if !capabilities.pmu_overflow {
                return Err(ScheduleError::MissingCapability {
                    capability: "PMU overflow interrupt",
                });
            }
        }
        ProgressMode::LegacyWallClock => {
            return Err(ScheduleError::UnsupportedProgressMode);
        }
    }
    Ok(())
}

/// Validate one complete schedule state without changing external state.
pub fn validate_state(state: &ScheduleState) -> Result<(), ScheduleError> {
    if state.schema_version != SCHEDULE_STATE_SCHEMA_VERSION {
        return Err(ScheduleError::UnsupportedSchema {
            found: state.schema_version,
        });
    }
    if state.num_vcpus == 0 {
        return Err(ScheduleError::InvalidConfiguration {
            reason: "num_vcpus must be positive",
        });
    }
    if state.num_vcpus > MAX_SCHEDULE_VCPUS {
        return Err(ScheduleError::TooManyVcpus {
            found: state.num_vcpus,
            limit: MAX_SCHEDULE_VCPUS,
        });
    }
    if state.quantum == 0 {
        return Err(ScheduleError::InvalidConfiguration {
            reason: "quantum must be positive",
        });
    }
    if state.runnable_vcpus.len() != state.num_vcpus
        || state.instruction_progress.len() != state.num_vcpus
    {
        return Err(ScheduleError::StateLengthMismatch);
    }
    if state.active_vcpu >= state.num_vcpus {
        return Err(ScheduleError::InvalidVcpu {
            vcpu: state.active_vcpu,
            num_vcpus: state.num_vcpus,
        });
    }
    validate_strategy(state.strategy)?;
    validate_mode_quantum(state.progress_mode, state.quantum, state.strategy)?;

    if state.halted {
        if state.runnable_vcpus.iter().any(|runnable| *runnable) {
            return Err(ScheduleError::InvalidConfiguration {
                reason: "halted state contains a runnable vCPU",
            });
        }
        if state.exact_step != ExactStepState::Inactive {
            return Err(ScheduleError::InvalidConfiguration {
                reason: "halted state has pending exact-step work",
            });
        }
        return Ok(());
    }

    if !state.runnable_vcpus[state.active_vcpu] {
        return Err(ScheduleError::ActiveVcpuNotRunnable {
            vcpu: state.active_vcpu,
        });
    }
    let progress = state.instruction_progress[state.active_vcpu];
    if progress > state.quantum_boundary {
        return Err(ScheduleError::ProgressOvershoot {
            boundary: state.quantum_boundary,
            observed: progress,
        });
    }
    match state.exact_step {
        ExactStepState::Inactive => {}
        ExactStepState::Active { remaining } => {
            if remaining == 0 {
                return Err(ScheduleError::InvalidConfiguration {
                    reason: "exact-step remainder must be positive",
                });
            }
            let expected_boundary =
                progress
                    .checked_add(remaining)
                    .ok_or(ScheduleError::CounterOverflow {
                        counter: "exact-step boundary",
                    })?;
            if expected_boundary != state.quantum_boundary {
                return Err(ScheduleError::InvalidConfiguration {
                    reason: "exact-step remainder does not reach the quantum boundary",
                });
            }
            if !matches!(state.progress_mode, ProgressMode::PmuAccelerated { .. }) {
                return Err(ScheduleError::InvalidConfiguration {
                    reason: "exact-step state requires PMU-accelerated mode",
                });
            }
        }
    }
    Ok(())
}

/// Compute one pure schedule transition.
pub fn transition(
    state: &ScheduleState,
    event: &ScheduleEvent,
) -> Result<PlannedScheduleTransition, ScheduleError> {
    validate_state(state)?;
    let pre_state_id = state.identity();
    let supplied_state_id = event.expected_state_id();
    if supplied_state_id != pre_state_id {
        return Err(ScheduleError::StaleState {
            expected: supplied_state_id,
            actual: pre_state_id,
        });
    }

    if let ScheduleEvent::HostEvent { kind, .. } = event {
        return Err(ScheduleError::HostOwnedInput { kind: *kind });
    }

    let mut next_state = state.clone();
    let (vcpu, observed_progress, runnable_changes, source) = match event {
        ScheduleEvent::GuestProgress {
            vcpu,
            observed_progress,
            runnable_changes,
            source,
            ..
        } => (*vcpu, *observed_progress, runnable_changes, *source),
        ScheduleEvent::HostEvent { .. } => unreachable!("host events return above"),
    };

    if vcpu >= state.num_vcpus {
        return Err(ScheduleError::InvalidVcpu {
            vcpu,
            num_vcpus: state.num_vcpus,
        });
    }
    if vcpu != state.active_vcpu {
        return Err(ScheduleError::TransitionMismatch {
            field: "event vCPU is not active",
        });
    }
    apply_runnable_changes(&mut next_state.runnable_vcpus, runnable_changes)?;

    let previous_progress = state.instruction_progress[vcpu];
    if observed_progress <= previous_progress {
        return Err(ScheduleError::StaleProgress {
            previous: previous_progress,
            observed: observed_progress,
        });
    }
    if observed_progress > state.quantum_boundary {
        return Err(ScheduleError::ProgressOvershoot {
            boundary: state.quantum_boundary,
            observed: observed_progress,
        });
    }
    validate_progress_increment(state, source, previous_progress, observed_progress)?;

    next_state.instruction_progress[vcpu] = observed_progress;
    next_state.sequence = state
        .sequence
        .checked_add(1)
        .ok_or(ScheduleError::CounterOverflow {
            counter: "schedule transition sequence",
        })?;

    let action = if !next_state.runnable_vcpus[vcpu] {
        finish_turn(
            &mut next_state,
            vcpu,
            observed_progress,
            SwitchReason::ActiveVcpuBlocked,
        )?
    } else if observed_progress == state.quantum_boundary {
        finish_turn(
            &mut next_state,
            vcpu,
            observed_progress,
            SwitchReason::QuantumBoundary,
        )?
    } else {
        continue_turn(&mut next_state, observed_progress)?
    };

    validate_state(&next_state)?;
    let post_state_id = next_state.identity();
    let record = ScheduleTransitionRecord {
        pre_state_id,
        event: event.clone(),
        action,
        post_state_id,
    };
    Ok(PlannedScheduleTransition { next_state, record })
}

/// Validate an untrusted compact record and return its recomputed post-state.
pub fn validate_transition_record(
    pre_state: &ScheduleState,
    record: &ScheduleTransitionRecord,
) -> Result<ScheduleState, ScheduleError> {
    validate_state(pre_state)?;
    let actual_pre_id = pre_state.identity();
    if actual_pre_id != record.pre_state_id {
        return Err(ScheduleError::IdentityMismatch {
            field: "pre_state_id",
            expected: record.pre_state_id,
            actual: actual_pre_id,
        });
    }

    let expected = transition(pre_state, &record.event)?;
    if expected.record.action != record.action {
        return Err(ScheduleError::TransitionMismatch { field: "action" });
    }
    if expected.record.post_state_id != record.post_state_id {
        return Err(ScheduleError::IdentityMismatch {
            field: "post_state_id",
            expected: record.post_state_id,
            actual: expected.record.post_state_id,
        });
    }
    Ok(expected.next_state)
}

/// Validate an untrusted bounded transition trace and return its final state.
pub fn validate_transition_trace(
    trace: &ScheduleTrace,
    limit: usize,
) -> Result<ScheduleState, ScheduleError> {
    if trace.records.len() > limit {
        return Err(ScheduleError::JournalCapacityExceeded { limit });
    }
    validate_state(&trace.initial_state)?;
    let actual_initial_id = trace.initial_state.identity();
    if actual_initial_id != trace.initial_state_id {
        return Err(ScheduleError::IdentityMismatch {
            field: "initial_state_id",
            expected: trace.initial_state_id,
            actual: actual_initial_id,
        });
    }

    let mut state = trace.initial_state.clone();
    for (index, record) in trace.records.iter().enumerate() {
        if state.identity() != record.pre_state_id {
            return Err(ScheduleError::TraceDiscontinuity { index });
        }
        state = validate_transition_record(&state, record)?;
    }
    Ok(state)
}

/// Bounded evidence journal with preflight reservation and atomic commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleJournal {
    limit: usize,
    initial_state: ScheduleState,
    state: ScheduleState,
    records: Vec<ScheduleTransitionRecord>,
    reservation: Option<u64>,
    next_reservation_id: u64,
}

impl ScheduleJournal {
    /// Create an empty journal from an already validated initial state.
    pub fn new(initial_state: ScheduleState, limit: usize) -> Result<Self, ScheduleError> {
        validate_state(&initial_state)?;
        Ok(Self {
            limit,
            initial_state: initial_state.clone(),
            state: initial_state,
            records: Vec::new(),
            reservation: None,
            next_reservation_id: 0,
        })
    }

    /// Reserve storage for one transition before any guest progress occurs.
    pub fn reserve(&mut self) -> Result<ScheduleReservation, ScheduleError> {
        if self.reservation.is_some() {
            return Err(ScheduleError::ReservationOutstanding);
        }
        if self.records.len() >= self.limit {
            return Err(ScheduleError::JournalCapacityExceeded { limit: self.limit });
        }
        self.records
            .try_reserve(1)
            .map_err(|_| ScheduleError::JournalAllocationFailed)?;
        let id = self.next_reservation_id;
        self.next_reservation_id =
            self.next_reservation_id
                .checked_add(1)
                .ok_or(ScheduleError::CounterOverflow {
                    counter: "schedule reservation sequence",
                })?;
        self.reservation = Some(id);
        Ok(ScheduleReservation { id })
    }

    /// Release a reservation after a shell failure that made no guest progress.
    pub fn release(&mut self, reservation: ScheduleReservation) -> Result<(), ScheduleError> {
        self.validate_reservation(&reservation)?;
        self.reservation = None;
        Ok(())
    }

    /// Validate and commit one record into its preflighted slot.
    pub fn commit(
        &mut self,
        reservation: ScheduleReservation,
        record: ScheduleTransitionRecord,
    ) -> Result<(), ScheduleError> {
        self.validate_reservation(&reservation)?;
        let next_state = validate_transition_record(&self.state, &record)?;
        self.records.push(record);
        self.state = next_state;
        self.reservation = None;
        Ok(())
    }

    /// Current state after all committed records.
    pub fn state(&self) -> &ScheduleState {
        &self.state
    }

    /// Return committed records.
    pub fn records(&self) -> &[ScheduleTransitionRecord] {
        &self.records
    }

    /// Drain a complete trace. No reservation can be outstanding.
    pub fn drain(&mut self) -> Result<ScheduleTrace, ScheduleError> {
        if self.reservation.is_some() {
            return Err(ScheduleError::ReservationOutstanding);
        }
        let trace = ScheduleTrace {
            initial_state: self.initial_state.clone(),
            initial_state_id: self.initial_state.identity(),
            records: std::mem::take(&mut self.records),
        };
        self.initial_state = self.state.clone();
        Ok(trace)
    }

    fn validate_reservation(&self, reservation: &ScheduleReservation) -> Result<(), ScheduleError> {
        if self.reservation == Some(reservation.id) {
            Ok(())
        } else {
            Err(ScheduleError::InvalidReservation)
        }
    }
}

fn validate_configuration(
    config: &SchedulerConfig,
    progress_mode: ProgressMode,
) -> Result<(), ScheduleError> {
    if config.num_vcpus == 0 {
        return Err(ScheduleError::InvalidConfiguration {
            reason: "num_vcpus must be positive",
        });
    }
    if config.num_vcpus > MAX_SCHEDULE_VCPUS {
        return Err(ScheduleError::TooManyVcpus {
            found: config.num_vcpus,
            limit: MAX_SCHEDULE_VCPUS,
        });
    }
    if config.quantum == 0 {
        return Err(ScheduleError::InvalidConfiguration {
            reason: "quantum must be positive",
        });
    }
    validate_strategy(config.strategy)?;
    validate_mode_quantum(progress_mode, config.quantum, config.strategy)
}

fn validate_strategy(strategy: SchedulingStrategy) -> Result<(), ScheduleError> {
    if let SchedulingStrategy::Randomized {
        min_quantum,
        max_quantum,
    } = strategy
    {
        if min_quantum == 0 {
            return Err(ScheduleError::InvalidConfiguration {
                reason: "randomized minimum quantum must be positive",
            });
        }
        if max_quantum <= min_quantum {
            return Err(ScheduleError::InvalidConfiguration {
                reason: "randomized maximum quantum must exceed minimum",
            });
        }
    }
    Ok(())
}

fn validate_mode_quantum(
    mode: ProgressMode,
    quantum: u64,
    strategy: SchedulingStrategy,
) -> Result<(), ScheduleError> {
    match mode {
        ProgressMode::ExactSingleStep => Ok(()),
        ProgressMode::PmuAccelerated { exact_step_margin } => {
            if exact_step_margin == 0 {
                return Err(ScheduleError::InvalidConfiguration {
                    reason: "PMU exact-step margin must be positive",
                });
            }
            let minimum_quantum = match strategy {
                SchedulingStrategy::RoundRobin => quantum,
                SchedulingStrategy::Randomized { min_quantum, .. } => min_quantum,
            };
            if exact_step_margin >= minimum_quantum {
                return Err(ScheduleError::InvalidConfiguration {
                    reason: "PMU exact-step margin must be smaller than every quantum",
                });
            }
            Ok(())
        }
        ProgressMode::LegacyWallClock => Err(ScheduleError::UnsupportedProgressMode),
    }
}

fn apply_runnable_changes(
    runnable_vcpus: &mut [bool],
    changes: &[RunnableChange],
) -> Result<(), ScheduleError> {
    if changes.len() > MAX_SCHEDULE_VCPUS {
        return Err(ScheduleError::TooManyVcpus {
            found: changes.len(),
            limit: MAX_SCHEDULE_VCPUS,
        });
    }
    let mut previous_vcpu = None;
    for change in changes {
        if change.vcpu >= runnable_vcpus.len() {
            return Err(ScheduleError::InvalidVcpu {
                vcpu: change.vcpu,
                num_vcpus: runnable_vcpus.len(),
            });
        }
        if previous_vcpu.is_some_and(|previous| previous >= change.vcpu) {
            return Err(ScheduleError::NonCanonicalRunnableChanges);
        }
        runnable_vcpus[change.vcpu] = change.runnable;
        previous_vcpu = Some(change.vcpu);
    }
    Ok(())
}

fn validate_progress_increment(
    state: &ScheduleState,
    source: ProgressSource,
    previous: u64,
    observed: u64,
) -> Result<(), ScheduleError> {
    let exact_expected = previous
        .checked_add(1)
        .ok_or(ScheduleError::CounterOverflow {
            counter: "exact guest instruction progress",
        })?;
    match (state.progress_mode, state.exact_step, source) {
        (
            ProgressMode::ExactSingleStep,
            ExactStepState::Inactive,
            ProgressSource::ExactSingleStep,
        )
        | (
            ProgressMode::PmuAccelerated { .. },
            ExactStepState::Active { .. },
            ProgressSource::ExactSingleStep,
        ) => {
            if observed != exact_expected {
                return Err(ScheduleError::ImpossibleExactProgress {
                    expected: exact_expected,
                    observed,
                });
            }
            Ok(())
        }
        (
            ProgressMode::PmuAccelerated { .. },
            ExactStepState::Inactive,
            ProgressSource::GuestInstructionPmu,
        ) => Ok(()),
        _ => Err(ScheduleError::ProgressSourceMismatch),
    }
}

fn continue_turn(
    next_state: &mut ScheduleState,
    observed_progress: u64,
) -> Result<ScheduleAction, ScheduleError> {
    match (next_state.progress_mode, next_state.exact_step) {
        (ProgressMode::PmuAccelerated { exact_step_margin }, ExactStepState::Inactive) => {
            let remaining = next_state.quantum_boundary - observed_progress;
            if remaining <= exact_step_margin {
                next_state.exact_step = ExactStepState::Active { remaining };
                Ok(ScheduleAction::EnterExactStep { remaining })
            } else {
                Ok(ScheduleAction::Continue)
            }
        }
        (ProgressMode::PmuAccelerated { .. }, ExactStepState::Active { remaining }) => {
            let next_remaining =
                remaining
                    .checked_sub(1)
                    .ok_or(ScheduleError::CounterOverflow {
                        counter: "exact-step remainder",
                    })?;
            if next_remaining == 0 {
                return Err(ScheduleError::InvalidConfiguration {
                    reason: "exact-step reached boundary without turn completion",
                });
            }
            next_state.exact_step = ExactStepState::Active {
                remaining: next_remaining,
            };
            Ok(ScheduleAction::Continue)
        }
        (ProgressMode::ExactSingleStep, ExactStepState::Inactive) => Ok(ScheduleAction::Continue),
        _ => Err(ScheduleError::ProgressSourceMismatch),
    }
}

fn finish_turn(
    next_state: &mut ScheduleState,
    from_vcpu: usize,
    from_progress: u64,
    reason: SwitchReason,
) -> Result<ScheduleAction, ScheduleError> {
    let declared_boundary = next_state.quantum_boundary;
    next_state.exact_step = ExactStepState::Inactive;
    let Some(to_vcpu) = next_runnable(&next_state.runnable_vcpus, from_vcpu) else {
        next_state.halted = true;
        return Ok(ScheduleAction::Halt);
    };

    let next_quantum = select_next_quantum(next_state);
    let next_boundary = next_state.instruction_progress[to_vcpu]
        .checked_add(next_quantum)
        .ok_or(ScheduleError::CounterOverflow {
            counter: "next quantum boundary",
        })?;
    next_state.active_vcpu = to_vcpu;
    next_state.quantum_boundary = next_boundary;
    next_state.halted = false;
    Ok(ScheduleAction::Switch {
        from_vcpu,
        to_vcpu,
        from_progress,
        declared_boundary,
        reason,
        next_boundary,
    })
}

fn next_runnable(runnable: &[bool], active: usize) -> Option<usize> {
    for offset in 1..=runnable.len() {
        let candidate = (active + offset) % runnable.len();
        if runnable[candidate] {
            return Some(candidate);
        }
    }
    None
}

fn select_next_quantum(state: &mut ScheduleState) -> u64 {
    match state.strategy {
        SchedulingStrategy::RoundRobin => state.quantum,
        SchedulingStrategy::Randomized {
            min_quantum,
            max_quantum,
        } => {
            let range = max_quantum - min_quantum;
            let mut rng = ChaCha20Rng::from_seed(state.rng_seed);
            rng.set_word_pos(state.rng_word_pos);
            let random = rng.next_u64();
            state.rng_word_pos = rng.get_word_pos();
            min_quantum + random % range
        }
    }
}

fn schedule_state_identity(state: &ScheduleState) -> ScheduleStateId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(SCHEDULE_STATE_DOMAIN);
    hash_u16(&mut hasher, state.schema_version);
    hash_usize(&mut hasher, state.num_vcpus);
    hash_usize(&mut hasher, state.active_vcpu);
    hash_usize(&mut hasher, state.runnable_vcpus.len());
    for runnable in &state.runnable_vcpus {
        hasher.update(&[u8::from(*runnable)]);
    }
    hash_usize(&mut hasher, state.instruction_progress.len());
    for progress in &state.instruction_progress {
        hash_u64(&mut hasher, *progress);
    }
    hash_u64(&mut hasher, state.quantum_boundary);
    hash_u64(&mut hasher, state.quantum);
    hash_strategy(&mut hasher, state.strategy);
    hasher.update(&state.rng_seed);
    hasher.update(&state.rng_word_pos.to_le_bytes());
    hash_progress_mode(&mut hasher, state.progress_mode);
    hash_exact_step(&mut hasher, state.exact_step);
    hash_u64(&mut hasher, state.sequence);
    hasher.update(&[u8::from(state.halted)]);
    ScheduleStateId(*hasher.finalize().as_bytes())
}

fn hash_strategy(hasher: &mut blake3::Hasher, strategy: SchedulingStrategy) {
    match strategy {
        SchedulingStrategy::RoundRobin => {
            hasher.update(&[0]);
        }
        SchedulingStrategy::Randomized {
            min_quantum,
            max_quantum,
        } => {
            hasher.update(&[1]);
            hash_u64(hasher, min_quantum);
            hash_u64(hasher, max_quantum);
        }
    }
}

fn hash_progress_mode(hasher: &mut blake3::Hasher, mode: ProgressMode) {
    match mode {
        ProgressMode::ExactSingleStep => {
            hasher.update(&[0]);
        }
        ProgressMode::PmuAccelerated { exact_step_margin } => {
            hasher.update(&[1]);
            hash_u64(hasher, exact_step_margin);
        }
        ProgressMode::LegacyWallClock => {
            hasher.update(&[2]);
        }
    }
}

fn hash_exact_step(hasher: &mut blake3::Hasher, exact_step: ExactStepState) {
    match exact_step {
        ExactStepState::Inactive => {
            hasher.update(&[0]);
        }
        ExactStepState::Active { remaining } => {
            hasher.update(&[1]);
            hash_u64(hasher, remaining);
        }
    }
}

fn hash_u16(hasher: &mut blake3::Hasher, value: u16) {
    hasher.update(&value.to_le_bytes());
}

fn hash_u64(hasher: &mut blake3::Hasher, value: u64) {
    hasher.update(&value.to_le_bytes());
}

fn hash_usize(hasher: &mut blake3::Hasher, value: usize) {
    let canonical = u64::try_from(value).expect("usize fits in u64 on supported targets");
    hash_u64(hasher, canonical);
}

fn deserialize_bounded_bools<'de, D>(deserializer: D) -> Result<Vec<bool>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_vec(deserializer, MAX_SCHEDULE_VCPUS, "vCPU runnable states")
}

fn deserialize_bounded_u64s<'de, D>(deserializer: D) -> Result<Vec<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_vec(deserializer, MAX_SCHEDULE_VCPUS, "vCPU progress states")
}

fn deserialize_bounded_runnable_changes<'de, D>(
    deserializer: D,
) -> Result<Vec<RunnableChange>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_vec(deserializer, MAX_SCHEDULE_VCPUS, "runnable changes")
}

fn deserialize_bounded_records<'de, D>(
    deserializer: D,
) -> Result<Vec<ScheduleTransitionRecord>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_vec(
        deserializer,
        DEFAULT_SCHEDULE_JOURNAL_LIMIT,
        "schedule transition records",
    )
}

fn deserialize_bounded_vec<'de, D, T>(
    deserializer: D,
    limit: usize,
    label: &'static str,
) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    struct BoundedVecVisitor<T> {
        limit: usize,
        label: &'static str,
        marker: std::marker::PhantomData<T>,
    }

    impl<'de, T> Visitor<'de> for BoundedVecVisitor<T>
    where
        T: DeserializeOwned,
    {
        type Value = Vec<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "at most {} {}", self.limit, self.label)
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let size_hint = sequence.size_hint().unwrap_or(0);
            if size_hint > self.limit {
                return Err(de::Error::custom(format_args!(
                    "{} count {} exceeds limit {}",
                    self.label, size_hint, self.limit
                )));
            }
            let mut values = Vec::with_capacity(size_hint);
            while let Some(value) = sequence.next_element()? {
                if values.len() >= self.limit {
                    return Err(de::Error::custom(format_args!(
                        "{} count exceeds limit {}",
                        self.label, self.limit
                    )));
                }
                values.push(value);
            }
            Ok(values)
        }
    }

    deserializer.deserialize_seq(BoundedVecVisitor {
        limit,
        label,
        marker: std::marker::PhantomData,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const VCPU_COUNT: usize = 2;
    const THREE_VCPUS: usize = 3;
    const QUANTUM: u64 = 4;
    const PMU_MARGIN: u64 = 2;
    const TEST_SEED: u64 = 42;
    const JOURNAL_LIMIT: usize = 1;

    fn config() -> SchedulerConfig {
        SchedulerConfig {
            num_vcpus: VCPU_COUNT,
            quantum: QUANTUM,
            strategy: SchedulingStrategy::RoundRobin,
            seed: TEST_SEED,
        }
    }

    fn exact_state() -> ScheduleState {
        ScheduleState::new(&config(), ProgressMode::ExactSingleStep).unwrap()
    }

    fn progress_event(state: &ScheduleState, progress: u64) -> ScheduleEvent {
        ScheduleEvent::GuestProgress {
            expected_state_id: state.identity(),
            vcpu: state.active_vcpu,
            observed_progress: progress,
            runnable_changes: Vec::new(),
            source: ProgressSource::ExactSingleStep,
        }
    }

    #[test]
    fn exact_progress_switches_only_at_declared_boundary() {
        let mut state = exact_state();
        for expected_progress in 1..QUANTUM {
            let planned = transition(&state, &progress_event(&state, expected_progress)).unwrap();
            assert_eq!(planned.record.action, ScheduleAction::Continue);
            assert_eq!(planned.next_state.active_vcpu, 0);
            state = planned.next_state;
        }

        let planned = transition(&state, &progress_event(&state, QUANTUM)).unwrap();
        assert_eq!(
            planned.record.action,
            ScheduleAction::Switch {
                from_vcpu: 0,
                to_vcpu: 1,
                from_progress: QUANTUM,
                declared_boundary: QUANTUM,
                reason: SwitchReason::QuantumBoundary,
                next_boundary: QUANTUM,
            }
        );
        assert_eq!(planned.next_state.active_vcpu, 1);
        assert_eq!(
            validate_transition_record(&state, &planned.record).unwrap(),
            planned.next_state
        );
    }

    #[test]
    fn runnable_order_is_part_of_the_pure_selection() {
        let mut test_config = config();
        test_config.num_vcpus = THREE_VCPUS;
        test_config.quantum = 1;
        let state = ScheduleState::new(&test_config, ProgressMode::ExactSingleStep).unwrap();
        let event = ScheduleEvent::GuestProgress {
            expected_state_id: state.identity(),
            vcpu: 0,
            observed_progress: 1,
            runnable_changes: vec![RunnableChange {
                vcpu: 1,
                runnable: false,
            }],
            source: ProgressSource::ExactSingleStep,
        };
        let planned = transition(&state, &event).unwrap();
        assert_eq!(planned.next_state.active_vcpu, 2);
    }

    #[test]
    fn stale_identity_and_forged_post_identity_are_rejected() {
        let state = exact_state();
        let mut stale_event = progress_event(&state, 1);
        if let ScheduleEvent::GuestProgress {
            expected_state_id, ..
        } = &mut stale_event
        {
            expected_state_id.0[0] ^= 1;
        }
        assert!(matches!(
            transition(&state, &stale_event),
            Err(ScheduleError::StaleState { .. })
        ));

        let mut record = transition(&state, &progress_event(&state, 1))
            .unwrap()
            .record;
        record.post_state_id.0[0] ^= 1;
        assert!(matches!(
            validate_transition_record(&state, &record),
            Err(ScheduleError::IdentityMismatch { .. })
        ));
    }

    #[test]
    fn invalid_vcpu_stale_progress_and_overshoot_fail_closed() {
        let state = exact_state();
        let invalid_vcpu = ScheduleEvent::GuestProgress {
            expected_state_id: state.identity(),
            vcpu: VCPU_COUNT,
            observed_progress: 1,
            runnable_changes: Vec::new(),
            source: ProgressSource::ExactSingleStep,
        };
        assert!(matches!(
            transition(&state, &invalid_vcpu),
            Err(ScheduleError::InvalidVcpu { .. })
        ));
        assert!(matches!(
            transition(&state, &progress_event(&state, 0)),
            Err(ScheduleError::StaleProgress { .. })
        ));
        assert!(matches!(
            transition(&state, &progress_event(&state, QUANTUM + 1)),
            Err(ScheduleError::ProgressOvershoot { .. })
        ));
    }

    #[test]
    fn exact_mode_rejects_impossible_progress_jump() {
        let state = exact_state();
        assert!(matches!(
            transition(&state, &progress_event(&state, PMU_MARGIN)),
            Err(ScheduleError::ImpossibleExactProgress { .. })
        ));
    }

    #[test]
    fn host_events_never_change_schedule_state() {
        let state = exact_state();
        for kind in [
            HostEventKind::SignalInterrupt,
            HostEventKind::WatchdogExpired,
            HostEventKind::ThreadWake,
        ] {
            let event = ScheduleEvent::HostEvent {
                expected_state_id: state.identity(),
                kind,
            };
            assert_eq!(
                transition(&state, &event),
                Err(ScheduleError::HostOwnedInput { kind })
            );
            assert_eq!(state.active_vcpu, 0);
            assert_eq!(state.sequence, 0);
        }
    }

    #[test]
    fn pmu_mode_enters_exact_step_and_rejects_overshoot() {
        let state = ScheduleState::new(
            &config(),
            ProgressMode::PmuAccelerated {
                exact_step_margin: PMU_MARGIN,
            },
        )
        .unwrap();
        let pmu_progress = ScheduleEvent::GuestProgress {
            expected_state_id: state.identity(),
            vcpu: 0,
            observed_progress: PMU_MARGIN,
            runnable_changes: Vec::new(),
            source: ProgressSource::GuestInstructionPmu,
        };
        let planned = transition(&state, &pmu_progress).unwrap();
        assert_eq!(
            planned.record.action,
            ScheduleAction::EnterExactStep {
                remaining: PMU_MARGIN,
            }
        );

        let overshoot = ScheduleEvent::GuestProgress {
            expected_state_id: state.identity(),
            vcpu: 0,
            observed_progress: QUANTUM + 1,
            runnable_changes: Vec::new(),
            source: ProgressSource::GuestInstructionPmu,
        };
        assert!(matches!(
            transition(&state, &overshoot),
            Err(ScheduleError::ProgressOvershoot { .. })
        ));
    }

    #[test]
    fn unavailable_and_legacy_progress_modes_fail_closed() {
        let no_pmu = ProgressCapabilities {
            exact_single_step: true,
            guest_instruction_pmu: false,
            pmu_overflow: false,
        };
        assert!(matches!(
            validate_progress_capabilities(
                ProgressMode::PmuAccelerated {
                    exact_step_margin: PMU_MARGIN,
                },
                no_pmu,
            ),
            Err(ScheduleError::MissingCapability { .. })
        ));
        assert_eq!(
            validate_progress_capabilities(ProgressMode::LegacyWallClock, no_pmu),
            Err(ScheduleError::UnsupportedProgressMode)
        );
        assert_eq!(
            ScheduleState::new(&config(), ProgressMode::LegacyWallClock),
            Err(ScheduleError::UnsupportedProgressMode)
        );
    }

    #[test]
    fn reservation_precedes_progress_and_capacity_failure_has_no_commit() {
        let state = exact_state();
        let first = transition(&state, &progress_event(&state, 1)).unwrap();
        let committed_state = first.next_state.clone();
        let mut journal = ScheduleJournal::new(state, JOURNAL_LIMIT).unwrap();
        let reservation = journal.reserve().unwrap();
        journal.commit(reservation, first.record).unwrap();
        let before = journal.clone();

        assert_eq!(
            journal.reserve(),
            Err(ScheduleError::JournalCapacityExceeded {
                limit: JOURNAL_LIMIT,
            })
        );
        assert_eq!(journal, before);
        assert_eq!(journal.state(), &committed_state);
    }

    #[test]
    fn no_progress_shell_failure_releases_reservation() {
        let state = exact_state();
        let mut journal = ScheduleJournal::new(state, JOURNAL_LIMIT).unwrap();
        let reservation = journal.reserve().unwrap();
        journal.release(reservation).unwrap();
        assert!(journal.reserve().is_ok());
        assert!(journal.records().is_empty());
    }

    #[test]
    fn snapshot_identity_binds_exact_step_and_rng_state() {
        let mut state = ScheduleState::new(
            &config(),
            ProgressMode::PmuAccelerated {
                exact_step_margin: PMU_MARGIN,
            },
        )
        .unwrap();
        let baseline = state.identity();
        state.exact_step = ExactStepState::Active {
            remaining: PMU_MARGIN,
        };
        state.instruction_progress[0] = PMU_MARGIN;
        let exact_step_id = state.identity();
        assert_ne!(baseline, exact_step_id);

        state.rng_word_pos = 1;
        assert_ne!(exact_step_id, state.identity());
    }

    #[test]
    fn trace_validation_rejects_replay_divergence() {
        let state = exact_state();
        let first = transition(&state, &progress_event(&state, 1)).unwrap();
        let second = transition(
            &first.next_state,
            &progress_event(&first.next_state, PMU_MARGIN),
        )
        .unwrap();
        let trace = ScheduleTrace {
            initial_state: state.clone(),
            initial_state_id: state.identity(),
            records: vec![first.record, second.record],
        };
        assert_eq!(
            validate_transition_trace(&trace, trace.records.len()).unwrap(),
            second.next_state
        );

        let mut divergent = trace;
        divergent.records[1].pre_state_id = state.identity();
        assert!(matches!(
            validate_transition_trace(&divergent, divergent.records.len()),
            Err(ScheduleError::TraceDiscontinuity { .. })
                | Err(ScheduleError::IdentityMismatch { .. })
        ));
    }

    #[test]
    fn serde_rejects_vcpu_vectors_above_allocation_bound() {
        let mut state = exact_state();
        state.num_vcpus = MAX_SCHEDULE_VCPUS + 1;
        state.runnable_vcpus = vec![true; MAX_SCHEDULE_VCPUS + 1];
        state.instruction_progress = vec![0; MAX_SCHEDULE_VCPUS + 1];
        let json = serde_json::to_vec(&state).unwrap();
        let decoded = serde_json::from_slice::<ScheduleState>(&json);
        assert!(decoded.is_err());
    }
}
