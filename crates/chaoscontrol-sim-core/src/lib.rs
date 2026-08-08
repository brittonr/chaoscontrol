#![forbid(unsafe_code)]

//! Pure deterministic decisions for ChaosControl simulations.
//!
//! This crate has no KVM, filesystem, wall-clock, environment, process, or
//! receipt-writing authority. Shell crates supply typed observations and apply
//! the returned commands.

pub mod boundary;
pub mod kernel;
pub mod scheduler;

pub use boundary::{
    validate_exchange, BoundaryError, BoundaryState, CommandExecutor, ExecutionCommand,
    ExitObservation, ValidatedExchange,
};
pub use kernel::{
    complete_round, plan_round, CanonicalEvent, CanonicalTrace, CoreVmStatus, RoundInput,
    RoundObservation, RoundPlan, SimulationKernelError,
};
