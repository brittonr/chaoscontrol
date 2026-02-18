//! Test property assertions — the core of ChaosControl's testing model.
//!
//! These assertions communicate properties to the VMM, which tracks them
//! across thousands of deterministic runs to find violations.
//!
//! # Assertion semantics
//!
//! | Function        | Single-run behavior                | Cross-run aggregation              |
//! |-----------------|------------------------------------|------------------------------------|
//! | [`always`]      | Fail if `cond` is ever false       | Fail if ANY run had false          |
//! | [`sometimes`]   | Record whether `cond` was true     | Fail if NO run ever had true       |
//! | [`reachable`]   | Record that this point was reached | Fail if NO run reached this point  |
//! | [`unreachable`] | Fail immediately                   | Fail if ANY run reached this point |
//!
//! # Assertion IDs
//!
//! Each assertion has a unique `id` (a `u32`) that identifies the
//! specific assertion site.  The oracle uses this to track per-site
//! satisfaction across runs.  Use [`location_id`] to derive an ID from
//! a string (typically `file!()` + `line!()`).

use crate::transport;
use chaoscontrol_protocol::*;

/// Derive a deterministic assertion ID from a location string.
///
/// Uses FNV-1a hash truncated to 32 bits.  Suitable for use with
/// `file!()` and `line!()` to create unique IDs per call site.
pub const fn location_id(location: &str) -> u32 {
    let bytes = location.as_bytes();
    let mut hash: u32 = 0x811c_9dc5; // FNV offset basis
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u32;
        hash = hash.wrapping_mul(0x0100_0193); // FNV prime
        i += 1;
    }
    hash
}

/// Assert that `cond` is true **every** time this point is reached.
///
/// If `cond` is false, this is an immediate test failure for the
/// current run.  Across all runs, the property fails if `cond` was
/// ever false.
///
/// # Example
///
/// ```rust,ignore
/// chaoscontrol_sdk::assert::always(
///     leader_id < cluster_size,
///     "leader ID is valid",
///     &[("leader", "2"), ("cluster_size", "3")],
/// );
/// ```
pub fn always(cond: bool, message: &str, details: &[(&str, &str)]) {
    let id = location_id(message);
    always_with_id(cond, id, message, details);
}

/// Like [`always`] but with an explicit assertion ID.
pub fn always_with_id(cond: bool, id: u32, message: &str, details: &[(&str, &str)]) {
    let flags = if cond { 0x01 } else { 0x00 };
    transport::hypercall(CMD_ASSERT_ALWAYS, flags, id, message, details);
}

/// Assert that `cond` is true **at least once** across all runs.
///
/// A single run where `cond` is false is fine — the assertion only
/// fails if `cond` is false in *every* run that reaches this point.
///
/// Use this for liveness properties: "eventually, something good
/// happens."
///
/// # Example
///
/// ```rust,ignore
/// chaoscontrol_sdk::assert::sometimes(
///     write_succeeded,
///     "at least one write succeeds",
///     &[("attempt", &attempt.to_string())],
/// );
/// ```
pub fn sometimes(cond: bool, message: &str, details: &[(&str, &str)]) {
    let id = location_id(message);
    sometimes_with_id(cond, id, message, details);
}

/// Like [`sometimes`] but with an explicit assertion ID.
pub fn sometimes_with_id(cond: bool, id: u32, message: &str, details: &[(&str, &str)]) {
    let flags = if cond { 0x01 } else { 0x00 };
    transport::hypercall(CMD_ASSERT_SOMETIMES, flags, id, message, details);
}

/// Assert that this code point is **reached at least once** across runs.
///
/// Useful for verifying that fault injection actually exercises the
/// error-handling paths you care about.
///
/// # Example
///
/// ```rust,ignore
/// if let Err(e) = disk.write(offset, &data) {
///     chaoscontrol_sdk::assert::reachable(
///         "disk write error path exercised",
///         &[("error", &e.to_string())],
///     );
///     // handle error...
/// }
/// ```
pub fn reachable(message: &str, details: &[(&str, &str)]) {
    let id = location_id(message);
    reachable_with_id(id, message, details);
}

/// Like [`reachable`] but with an explicit assertion ID.
pub fn reachable_with_id(id: u32, message: &str, details: &[(&str, &str)]) {
    transport::hypercall(CMD_ASSERT_REACHABLE, 0x01, id, message, details);
}

/// Assert that this code point is **never reached** in any run.
///
/// If execution reaches this point, the test fails immediately.
///
/// # Example
///
/// ```rust,ignore
/// match state {
///     State::Valid => { /* ok */ }
///     State::Invalid => {
///         chaoscontrol_sdk::assert::unreachable(
///             "reached invalid state",
///             &[("state", &format!("{:?}", state))],
///         );
///     }
/// }
/// ```
pub fn unreachable(message: &str, details: &[(&str, &str)]) {
    let id = location_id(message);
    unreachable_with_id(id, message, details);
}

/// Like [`unreachable`] but with an explicit assertion ID.
pub fn unreachable_with_id(id: u32, message: &str, details: &[(&str, &str)]) {
    transport::hypercall(CMD_ASSERT_UNREACHABLE, 0x00, id, message, details);
}

// ═══════════════════════════════════════════════════════════════════════
//  Convenience macros
// ═══════════════════════════════════════════════════════════════════════

/// Assert-always with automatic source location ID.
///
/// ```rust,ignore
/// cc_assert_always!(leader_id < 3, "valid leader", ("leader", "2"));
/// ```
#[macro_export]
macro_rules! cc_assert_always {
    ($cond:expr, $msg:expr $(, ($k:expr, $v:expr))* $(,)?) => {{
        const _ID: u32 = $crate::assert::location_id(
            concat!(file!(), ":", line!(), ":", $msg)
        );
        $crate::assert::always_with_id($cond, _ID, $msg, &[$(($k, $v)),*]);
    }};
}

/// Assert-sometimes with automatic source location ID.
#[macro_export]
macro_rules! cc_assert_sometimes {
    ($cond:expr, $msg:expr $(, ($k:expr, $v:expr))* $(,)?) => {{
        const _ID: u32 = $crate::assert::location_id(
            concat!(file!(), ":", line!(), ":", $msg)
        );
        $crate::assert::sometimes_with_id($cond, _ID, $msg, &[$(($k, $v)),*]);
    }};
}

/// Assert-reachable with automatic source location ID.
#[macro_export]
macro_rules! cc_assert_reachable {
    ($msg:expr $(, ($k:expr, $v:expr))* $(,)?) => {{
        const _ID: u32 = $crate::assert::location_id(
            concat!(file!(), ":", line!(), ":", $msg)
        );
        $crate::assert::reachable_with_id(_ID, $msg, &[$(($k, $v)),*]);
    }};
}

/// Assert-unreachable with automatic source location ID.
#[macro_export]
macro_rules! cc_assert_unreachable {
    ($msg:expr $(, ($k:expr, $v:expr))* $(,)?) => {{
        const _ID: u32 = $crate::assert::location_id(
            concat!(file!(), ":", line!(), ":", $msg)
        );
        $crate::assert::unreachable_with_id(_ID, $msg, &[$(($k, $v)),*]);
    }};
}

// ═══════════════════════════════════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_id_deterministic() {
        let a = location_id("foo.rs:42:valid leader");
        let b = location_id("foo.rs:42:valid leader");
        assert_eq!(a, b);
    }

    #[test]
    fn location_id_different_for_different_inputs() {
        let a = location_id("foo.rs:42:msg1");
        let b = location_id("foo.rs:43:msg2");
        assert_ne!(a, b);
    }

    #[test]
    fn location_id_empty_string() {
        let id = location_id("");
        // Should be FNV offset basis since no bytes are hashed
        assert_eq!(id, 0x811c_9dc5);
    }

    #[test]
    fn location_id_single_char_difference() {
        let a = location_id("a");
        let b = location_id("b");
        assert_ne!(a, b);
    }
}
