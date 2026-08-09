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
pub use protocol_simulation::{
    schedule_next_protocol_event, verify_repeatable_transition, PendingProtocolEvent,
    ProtocolAdapter, ProtocolEventSchedulerState, ProtocolFact, ProtocolFactKind, ProtocolIdentity,
    ProtocolRngPolicy, ProtocolScheduleDecision, ProtocolScheduleError, ProtocolScheduleRef,
    ProtocolSchedulerPolicy, ProtocolSimulationConfig, ProtocolSimulationEvidenceClass,
    ProtocolSimulationReceipt, ProtocolTransition, ProtocolTransitionCheckError,
    ProtocolTransitionCheckResult, ProtocolTransitionInput, ProtocolVirtualClockPolicy,
    PROTOCOL_SIMULATION_CONFIG_SCHEMA, PROTOCOL_SIMULATION_RECEIPT_SCHEMA,
};
pub use snapshot::{CoreSnapshotError, SimulationCoreSnapshot, CORE_SNAPSHOT_SCHEMA_VERSION};
