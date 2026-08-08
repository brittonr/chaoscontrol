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
pub use snapshot::{CoreSnapshotError, SimulationCoreSnapshot, CORE_SNAPSHOT_SCHEMA_VERSION};
