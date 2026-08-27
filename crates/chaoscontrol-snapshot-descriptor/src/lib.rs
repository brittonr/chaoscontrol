#![allow(
    unknown_lints,
    reason = "stable builds must accept lint names that Octet registers only during its analysis pass"
)]

//! Pure portable descriptor contracts for exact ChaosControl snapshots.
//!
//! This crate owns deterministic descriptor identity, closure admission,
//! destination preflight, detached restore observations, locator sidecars, and
//! refs-only consumer projections. It performs no I/O and grants no authority.

mod canonical;
mod model;
mod observations;
mod validation;

pub use canonical::{
    descriptor_identity, destination_identity, digest_bytes, preflight_identity, verify_content,
};
pub use model::*;
pub use observations::*;
pub use validation::{
    expected_state_owners, preflight, validate_consumer_reference, validate_descriptor,
    validate_locator_sidecar, validate_payload_closure, validate_restore_receipt, DescriptorError,
};
