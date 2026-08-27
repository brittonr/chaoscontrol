#![allow(unknown_lints)]
#![allow(
    non_trait_imports,
    reason = "the consumer shell maps explicit ChaosControl and VM Cohort protocol types"
)]
#![allow(
    path_segment_repetition,
    reason = "consumer and shared boundary names remain explicit and searchable"
)]

//! ChaosControl-owned adapter over exact VM Cohort mechanics.
//!
//! Fault, scheduler, assertion, coverage, exploration, replay, guest, and
//! evidence meaning remains in ChaosControl.

mod execution;
mod mapping;
mod model;
mod parity;
mod selection;

pub use execution::*;
pub use mapping::*;
pub use model::*;
pub use parity::*;
pub use selection::*;

#[cfg(test)]
mod tests;
