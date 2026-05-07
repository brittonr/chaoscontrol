//! Prelude — convenient re-exports for ChaosControl SDK.
//!
//! ```rust,ignore
//! use chaoscontrol_sdk::prelude::*;
//! use serde_json::json;
//!
//! chaoscontrol_init();
//! setup_complete(&json!({"nodes": 3}));
//!
//! cc_assert_always!(leader_id < 3, "valid leader");
//! cc_assert_sometimes!(write_ok, "write succeeded");
//!
//! let choice = random_choice(10);
//! ```

// ── Init ─────────────────────────────────────────────────────────────
pub use crate::{chaoscontrol_init, is_in_vm, is_local_output};

// ── Guest runtime ────────────────────────────────────────────────────
#[cfg(feature = "full")]
pub use crate::runtime::guest_init;

// ── Assertion functions ──────────────────────────────────────────────
pub use crate::assert::{
    always, always_or_unreachable, always_or_unreachable_with_id, always_with_id, assert_raw,
    assert_raw_with_id, location_id, reachable, reachable_with_id, sometimes, sometimes_with_id,
    unreachable, unreachable_with_id, AssertionKind,
};

// ── Assertion macros ─────────────────────────────────────────────────
pub use crate::{
    // Core
    cc_assert_always,
    cc_assert_always_category,
    // Always comparisons
    cc_assert_always_eq,
    cc_assert_always_err,
    cc_assert_always_ge,
    cc_assert_always_gt,
    cc_assert_always_le,
    cc_assert_always_lt,
    cc_assert_always_ne,
    // Result assertions
    cc_assert_always_ok,
    cc_assert_always_or_unreachable,
    cc_assert_always_some,
    // Implication
    cc_assert_implies,
    cc_assert_raw,
    cc_assert_reachable,
    cc_assert_reachable_category,
    cc_assert_sometimes,
    cc_assert_sometimes_category,
    // Sometimes comparisons
    cc_assert_sometimes_eq,
    cc_assert_sometimes_err,
    cc_assert_sometimes_ge,
    cc_assert_sometimes_gt,
    cc_assert_sometimes_le,
    cc_assert_sometimes_lt,
    cc_assert_sometimes_ne,
    cc_assert_sometimes_ok,
    cc_assert_sometimes_some,
    cc_assert_unreachable,
};

// ── Guidance ─────────────────────────────────────────────────────────
pub use crate::guidance::{guidance, guidance_with_id};

// ── Lifecycle ────────────────────────────────────────────────────────
pub use crate::lifecycle::{send_event, setup_complete};

// ── Random ───────────────────────────────────────────────────────────
pub use crate::random::{fill_bytes, get_random, random_choice};

#[cfg(feature = "full")]
pub use crate::random::{random_choice_from, ChaosControlRng};

// ── Workload harness ─────────────────────────────────────────────────
#[cfg(feature = "full")]
pub use crate::workload::{LocalDryRunReport, WorkloadHarness};

// ── Re-export serde_json for json!() macro convenience ───────────────
#[cfg(feature = "full")]
pub use serde_json;
#[cfg(feature = "full")]
pub use serde_json::json;
