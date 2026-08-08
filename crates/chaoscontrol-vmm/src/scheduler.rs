//! Compatibility facade for the pure deterministic scheduler.
//!
//! `chaoscontrol-sim-core` owns schedule decisions and evidence. This module
//! keeps the established VMM paths stable for shell callers.

pub use chaoscontrol_sim_core::scheduler::*;
