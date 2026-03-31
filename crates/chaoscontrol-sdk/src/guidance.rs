//! Guidance — numeric distance-to-violation hints for the explorer.
//!
//! The explorer uses coverage to decide *where* to mutate, but guidance
//! tells it *how close* the guest is to violating a property.  Smaller
//! distance values mean closer to violation; 0.0 means violated.
//!
//! Guidance is tied to assertion sites: each call identifies the
//! assertion by ID (or message string), and the fault engine stores the
//! last-reported distance per assertion.
//!
//! # Example
//!
//! ```rust,ignore
//! use chaoscontrol_sdk::prelude::*;
//!
//! // Property: cluster should always have at most 1 leader
//! let leader_count = nodes.iter().filter(|n| n.is_leader()).count();
//! cc_assert_always!(leader_count <= 1, "at most one leader");
//!
//! // Guidance: how far from violating? 0 = violated, 1+ = safe
//! guidance("at most one leader", (leader_count as f64 - 1.0).max(0.0));
//! ```
//!
//! # Semantics
//!
//! - `distance = 0.0` means the property is currently violated.
//! - `distance > 0.0` means the property holds; larger = farther from violation.
//! - `NaN` is stored as-is; consumers treat it as "no guidance."
//! - Negative values are permitted; consumers may clamp to 0.

#[cfg(feature = "full")]
use crate::assert::location_id;
#[cfg(feature = "full")]
use crate::transport;

/// Send a guidance hint for a named assertion.
///
/// The assertion ID is derived from `message` via [`location_id`],
/// matching the convention used by assertion macros.  For best results,
/// use the same message string as the corresponding `cc_assert_*!` call.
///
/// # Arguments
///
/// * `message` — assertion message string (used to derive the ID)
/// * `distance` — distance-to-violation (0.0 = violated, larger = farther)
#[cfg(feature = "full")]
pub fn guidance(message: &str, distance: f64) {
    let id = location_id(message);
    guidance_with_id(id, distance);
}

/// Send a guidance hint with an explicit assertion ID.
///
/// Use this when you maintain your own ID scheme or want to match
/// an assertion created with `*_with_id` functions.
#[cfg(feature = "full")]
pub fn guidance_with_id(id: u32, distance: f64) {
    transport::hypercall_guidance(id, distance);
}

// ── No-op stubs ─────────────────────────────────────────────────────

/// No-op stub for `guidance` when SDK features are disabled.
#[cfg(not(feature = "full"))]
pub fn guidance(_message: &str, _distance: f64) {}

/// No-op stub for `guidance_with_id` when SDK features are disabled.
#[cfg(not(feature = "full"))]
pub fn guidance_with_id(_id: u32, _distance: f64) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guidance_compiles_full_mode() {
        // Just verify the function signatures are callable.
        guidance("test property", 1.0);
        guidance_with_id(42, 0.0);
        guidance("nan test", f64::NAN);
        guidance("negative test", -1.0);
    }
}
