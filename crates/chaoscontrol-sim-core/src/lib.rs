#![forbid(unsafe_code)]

//! Pure deterministic decisions for ChaosControl simulations.
//!
//! This crate has no KVM, filesystem, wall-clock, environment, process, or
//! receipt-writing authority. Shell crates supply typed observations and apply
//! the returned commands.

pub mod boundary;
pub mod fault;
pub mod kernel;
pub mod network;
pub mod protocol_fault;
pub mod protocol_receipt;
pub mod protocol_replay;
pub mod protocol_simulation;
pub mod scheduler;
pub mod snapshot;

pub use boundary::{
    validate_exchange, BoundaryError, BoundaryState, CommandExecutor, ExecutionCommand,
    ExitObservation, ValidatedExchange,
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
pub use snapshot::{CoreSnapshotError, SimulationCoreSnapshot, CORE_SNAPSHOT_SCHEMA_VERSION};
