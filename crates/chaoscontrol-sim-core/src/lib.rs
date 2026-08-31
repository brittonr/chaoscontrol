#![forbid(unsafe_code)]

//! Pure deterministic decisions for ChaosControl simulations.
//!
//! This crate has no KVM, filesystem, wall-clock, environment, process, or
//! receipt-writing authority. Shell crates supply typed observations and apply
//! the returned commands.

pub mod boundary;
pub mod causality;
pub mod fault;
pub mod findability;
pub mod guest_determinism;
pub mod kernel;
pub mod network;
pub mod protocol_fault;
pub mod protocol_receipt;
pub mod protocol_replay;
pub mod protocol_simulation;
pub mod runtime_capacity;
pub mod scheduler;
pub mod snapshot;

pub use boundary::{
    validate_exchange, BoundaryError, BoundaryState, CommandExecutor, ExecutionCommand,
    ExitObservation, ValidatedExchange,
};
pub use guest_determinism::{
    build_guest_determinism_profile, compare_guest_determinism_probes, derive_boot_entropy_seed,
    derive_layout_binding, encode_linux_rng_seed_setup_data, validate_guest_determinism_probe,
    validate_guest_determinism_profile, GuestClockMode, GuestDeterminismDriftReport,
    GuestDeterminismError, GuestDeterminismInput, GuestDeterminismProbe, GuestDeterminismProfile,
    GuestDeterminismSurface, BOOT_ENTROPY_SEED_BYTES, GUEST_DETERMINISM_DRIFT_SCHEMA,
    GUEST_DETERMINISM_PROBE_SCHEMA, GUEST_DETERMINISM_PROFILE_SCHEMA, LINUX_RNG_SETUP_DATA_BYTES,
    LINUX_SETUP_DATA_HEADER_BYTES, LINUX_SETUP_RNG_SEED_TYPE,
};
pub use kernel::{
    complete_round, guest_artifact_set_identity, plan_round, simulation_config_identity,
    CanonicalEvent, CanonicalTrace, CoreVmStatus, RoundInput, RoundObservation, RoundPlan,
    SimulationKernelError,
};
pub use protocol_fault::{
    plan_protocol_fault, ProtocolFaultContext, ProtocolFaultDecision, ProtocolFaultEffect,
    ProtocolFaultError, ProtocolFaultHook, ScheduledProtocolFault,
};
pub use protocol_receipt::{
    build_protocol_simulation_receipt, protocol_simulation_config_digest,
    validate_protocol_simulation_receipt, ProtocolReceiptError,
};
pub use protocol_replay::{
    compare_protocol_simulation_receipts, ProtocolReplayComparison, ProtocolReplayMismatch,
    ProtocolReplayMismatchClass,
};
pub use protocol_simulation::{
    schedule_next_protocol_event, validate_protocol_effect_requests, verify_repeatable_transition,
    PendingProtocolEvent, ProtocolAdapter, ProtocolEffectRequest, ProtocolEffectValidationError,
    ProtocolEventSchedulerState, ProtocolFact, ProtocolFactKind, ProtocolIdentity,
    ProtocolRngPolicy, ProtocolScheduleDecision, ProtocolScheduleError, ProtocolScheduleRef,
    ProtocolSchedulerPolicy, ProtocolSimulationConfig, ProtocolSimulationEvidenceClass,
    ProtocolSimulationReceipt, ProtocolTransition, ProtocolTransitionCheckError,
    ProtocolTransitionCheckResult, ProtocolTransitionInput, ProtocolUnboundNondeterminism,
    ProtocolVirtualClockPolicy, PROTOCOL_SIMULATION_CONFIG_SCHEMA,
    PROTOCOL_SIMULATION_RECEIPT_SCHEMA,
};
pub use runtime_capacity::{
    plan_runtime_capacity, runtime_capacity_plan_identity, validate_runtime_capacity_observations,
    CapacityField, CapacityLease, CapacityPoolState, CapacitySlotState, CapacityUsageObservation,
    RuntimeCapacityClaims, RuntimeCapacityError, RuntimeCapacityLimits,
    RuntimeCapacityObservationError, RuntimeCapacityObservations, RuntimeCapacityPlan,
    RuntimeCapacityStartupResult, ScratchClassLimit, RUNTIME_CAPACITY_OBSERVATIONS_SCHEMA,
    RUNTIME_CAPACITY_PLAN_SCHEMA,
};
pub use snapshot::{CoreSnapshotError, SimulationCoreSnapshot, CORE_SNAPSHOT_SCHEMA_VERSION};
