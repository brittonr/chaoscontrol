//! Guest-side SDK for ChaosControl deterministic simulation testing.
//!
//! This crate provides an [Antithesis](https://antithesis.com)-style API for
//! annotating software under test with **assertions**, **lifecycle events**,
//! and **guided randomness**.  The VMM collects these signals to drive fault
//! injection and property checking across thousands of deterministic runs.
//!
//! # Quick start
//!
//! ```rust,ignore
//! use chaoscontrol_sdk::prelude::*;
//!
//! chaoscontrol_init();
//!
//! // Signal that setup is done — faults may begin
//! setup_complete(&[("nodes", "3")]);
//!
//! // Property: leader must always be valid when checked
//! cc_assert_always!(leader_id < num_nodes, "valid leader");
//!
//! // Property: eventually at least one write succeeds
//! cc_assert_sometimes!(write_ok, "write succeeded");
//!
//! // Guided random: VMM controls this for exploration
//! let action = random_choice(3); // 0, 1, or 2
//! ```
//!
//! # Modes of operation
//!
//! The SDK has three runtime modes, selected automatically:
//!
//! | Mode | Condition | Behavior |
//! |------|-----------|----------|
//! | **VM** | Running inside a ChaosControl VM | Full transport via vmcall |
//! | **Local output** | `CHAOSCONTROL_SDK_LOCAL_OUTPUT` env var set | Log assertions to JSON file |
//! | **No-op** | Outside VM, no env var | Silently discard |
//!
//! Additionally, building with `default-features = false` compiles the
//! SDK as zero-cost no-op stubs — suitable for production builds where
//! you want assertions evaluated but not emitted.
//!
//! # Features
//!
//! | Feature | Default | Description |
//! |---------|---------|-------------|
//! | `full` | ✅ | Everything: VM transport, local fallback, RngCore, JSON output |
//!
//! To disable all SDK functionality at build time:
//! ```toml
//! chaoscontrol-sdk = { path = "...", default-features = false }
//! ```

#![cfg_attr(not(feature = "full"), no_std)]

// The host test harness needs allocation. The minimal library still uses no_std.
#[cfg(all(test, not(feature = "full")))]
extern crate std;

pub mod assert;
#[cfg(feature = "full")]
mod assertion_catalog;
#[cfg(feature = "full")]
mod bounded_json;
pub mod coverage;
#[cfg(feature = "full")]
pub mod details;
pub mod lifecycle;
#[cfg(feature = "full")]
pub mod marker;
pub mod prelude;
#[cfg(feature = "full")]
pub mod process;
#[cfg(feature = "full")]
pub mod protocol_observation;
pub mod random;
pub mod resources;
mod transport;
#[cfg(feature = "full")]
pub mod workload;

/// Re-export serde_json for use by macros via `$crate::serde_json`.
#[cfg(feature = "full")]
#[doc(hidden)]
pub use serde_json;

#[cfg(feature = "full")]
mod internal;
#[cfg(feature = "full")]
mod local_json_security;

#[cfg(feature = "full")]
pub mod kcov;

#[cfg(feature = "full")]
pub mod runtime;
#[cfg(feature = "full")]
pub mod supervisor;

/// Initialize the ChaosControl SDK.
///
/// Detects the runtime mode (VM transport, local JSON output, or no-op)
/// and performs any one-time setup.  This should be called as early as
/// possible in `main()`.
///
/// If not called explicitly, the SDK will initialize lazily on first use.
/// However, calling it early ensures the [assertion catalog] is registered
/// before any code paths are exercised — otherwise, assertions in
/// never-reached code won't be reported.
///
/// Safe to call multiple times (idempotent).
///
/// [assertion catalog]: https://antithesis.com/docs/properties_assertions/properties/
///
/// # Example
///
/// ```rust,ignore
/// fn main() {
///     chaoscontrol_sdk::chaoscontrol_init();
///     // ... rest of your program
/// }
/// ```
pub fn chaoscontrol_init() {
    #[cfg(feature = "full")]
    crate::internal::init();
}

/// Returns `true` if the SDK is running inside a ChaosControl VM.
///
/// Useful for conditional behavior — e.g., enabling more verbose
/// logging or additional invariant checks when under test.
///
/// Always returns `false` when built with `default-features = false`.
pub fn is_in_vm() -> bool {
    #[cfg(feature = "full")]
    {
        crate::internal::is_in_vm()
    }
    #[cfg(not(feature = "full"))]
    {
        false
    }
}

/// Returns `true` if local JSON output is enabled.
///
/// This is set by the `CHAOSCONTROL_SDK_LOCAL_OUTPUT` environment
/// variable.  When true, assertions and lifecycle events are logged
/// to a JSON file following the Antithesis fallback schema.
pub fn is_local_output() -> bool {
    #[cfg(feature = "full")]
    {
        crate::internal::is_local_output()
    }
    #[cfg(not(feature = "full"))]
    {
        false
    }
}
