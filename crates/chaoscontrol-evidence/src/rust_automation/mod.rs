//! Rust-owned product automation cores.
//!
//! These modules parse supplied values and return deterministic outputs. CLI
//! binaries own file, process, clock, and terminal effects.

pub mod accepted_dogfood;
pub mod audit;
pub mod bounded_input;
pub mod dogfood_receipt;
pub mod dogfood_summary;
pub mod local_kvm;
pub mod readiness_receipt;
pub mod scaffold;
pub mod source_guard;
pub mod time;
pub mod vm_determinism;
