//! Test property assertions — the core of ChaosControl's testing model.
//!
//! These assertions communicate properties to the VMM, which tracks them
//! across thousands of deterministic runs to find violations.
//!
//! # Assertion semantics
//!
//! | Function                 | Single-run behavior                | Cross-run aggregation              |
//! |--------------------------|------------------------------------|------------------------------------|
//! | [`always`]               | Fail if `cond` is ever false       | Fail if ANY run had false          |
//! | [`sometimes`]            | Record whether `cond` was true     | Fail if NO run ever had true       |
//! | [`reachable`]            | Record that this point was reached | Fail if NO run reached this point  |
//! | [`unreachable`]          | Fail immediately                   | Fail if ANY run reached this point |
//! | [`always_or_unreachable`]| Like `always`, but unreachable on false | Immediate failure + tracked |
//!
//! # Assertion IDs
//!
//! Each assertion has a unique `id` (a `u32`) that identifies the
//! specific assertion site.  The oracle uses this to track per-site
//! satisfaction across runs.  Use [`location_id`] to derive an ID from
//! a string (typically `file!()` + `line!()`).

use crate::transport;
use chaoscontrol_protocol::*;

// ═══════════════════════════════════════════════════════════════════════
//  Assertion catalog (compile-time registration via linkme)
// ═══════════════════════════════════════════════════════════════════════

/// Catalog kind constants for `CatalogEntry::kind`.
pub const CATALOG_KIND_ALWAYS: u8 = 0;
pub const CATALOG_KIND_SOMETIMES: u8 = 1;
pub const CATALOG_KIND_REACHABLE: u8 = 2;
pub const CATALOG_KIND_UNREACHABLE: u8 = 3;

/// A registered assertion site, created at link time by assertion macros.
///
/// Every `cc_assert_*!` macro emits a static `CatalogEntry` into the
/// `ASSERTION_CATALOG` distributed slice.  At initialization, the SDK
/// sends all catalog entries to the VMM so the oracle can distinguish
/// "never hit" from "never registered."
pub struct CatalogEntry {
    /// Unique assertion ID (FNV-1a hash of location string).
    pub id: u32,
    /// Human-readable assertion message.
    pub message: &'static str,
    /// Kind of assertion: one of `CATALOG_KIND_*` constants.
    pub kind: u8,
    /// Source file where the assertion macro was invoked.
    pub file: &'static str,
    /// Source line where the assertion macro was invoked.
    pub line: u32,
}

// Safety: CatalogEntry only contains &'static references and Copy types.
unsafe impl Send for CatalogEntry {}
unsafe impl Sync for CatalogEntry {}

/// Distributed slice collecting all assertion sites across the binary.
///
/// Populated at link time by `cc_assert_*!` macros.  Iterated during
/// `chaoscontrol_init()` to register every assertion site with the VMM.
#[cfg(feature = "full")]
#[linkme::distributed_slice]
pub static ASSERTION_CATALOG: [CatalogEntry];

/// Send all catalog entries to the VMM (or local output).
///
/// Called once during `chaoscontrol_init()`.  Each entry becomes a
/// `CMD_ASSERT_CATALOG` hypercall so the oracle knows about every
/// assertion site — including ones that are never reached.
#[cfg(feature = "full")]
pub(crate) fn emit_catalog() {
    for entry in ASSERTION_CATALOG.iter() {
        let kind_byte = entry.kind;
        // Encode location as JSON details: {"file":"foo.rs","line":42}
        let details = serde_json::json!({
            "file": entry.file,
            "line": entry.line,
        });
        let json_bytes = serde_json::to_vec(&details).unwrap_or_else(|_| b"{}".to_vec());
        transport::hypercall(
            CMD_ASSERT_CATALOG,
            kind_byte,
            entry.id,
            entry.message,
            &json_bytes,
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Location ID
// ═══════════════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════════════
//  JSON serialization helper
// ═══════════════════════════════════════════════════════════════════════

/// Serialize details to JSON bytes. Only available with `full` feature.
#[cfg(feature = "full")]
fn to_json_bytes(details: &serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(details).unwrap_or_else(|_| b"{}".to_vec())
}

// ═══════════════════════════════════════════════════════════════════════
//  Core assertions (full mode)
// ═══════════════════════════════════════════════════════════════════════

/// Assert that `cond` is true **every** time this point is reached.
///
/// If `cond` is false, this is an immediate test failure for the
/// current run.  Across all runs, the property fails if `cond` was
/// ever false.
///
/// # Example
///
/// ```rust,ignore
/// use serde_json::json;
/// chaoscontrol_sdk::assert::always(
///     leader_id < cluster_size,
///     "leader ID is valid",
///     &json!({"leader": leader_id, "cluster_size": cluster_size}),
/// );
/// ```
#[cfg(feature = "full")]
pub fn always(cond: bool, message: &str, details: &serde_json::Value) {
    let id = location_id(message);
    always_with_id(cond, id, message, details);
}

/// Like [`always`] but with an explicit assertion ID.
#[cfg(feature = "full")]
pub fn always_with_id(cond: bool, id: u32, message: &str, details: &serde_json::Value) {
    let flags = if cond { 0x01 } else { 0x00 };
    let json_bytes = to_json_bytes(details);
    transport::hypercall(CMD_ASSERT_ALWAYS, flags, id, message, &json_bytes);
}

/// Assert that `cond` is true **at least once** across all runs.
///
/// A single run where `cond` is false is fine — the assertion only
/// fails if `cond` is false in *every* run that reaches this point.
///
/// Use this for liveness properties: "eventually, something good
/// happens."
#[cfg(feature = "full")]
pub fn sometimes(cond: bool, message: &str, details: &serde_json::Value) {
    let id = location_id(message);
    sometimes_with_id(cond, id, message, details);
}

/// Like [`sometimes`] but with an explicit assertion ID.
#[cfg(feature = "full")]
pub fn sometimes_with_id(cond: bool, id: u32, message: &str, details: &serde_json::Value) {
    let flags = if cond { 0x01 } else { 0x00 };
    let json_bytes = to_json_bytes(details);
    transport::hypercall(CMD_ASSERT_SOMETIMES, flags, id, message, &json_bytes);
}

/// Assert that this code point is **reached at least once** across runs.
#[cfg(feature = "full")]
pub fn reachable(message: &str, details: &serde_json::Value) {
    let id = location_id(message);
    reachable_with_id(id, message, details);
}

/// Like [`reachable`] but with an explicit assertion ID.
#[cfg(feature = "full")]
pub fn reachable_with_id(id: u32, message: &str, details: &serde_json::Value) {
    let json_bytes = to_json_bytes(details);
    transport::hypercall(CMD_ASSERT_REACHABLE, 0x01, id, message, &json_bytes);
}

/// Assert that this code point is **never reached** in any run.
#[cfg(feature = "full")]
pub fn unreachable(message: &str, details: &serde_json::Value) {
    let id = location_id(message);
    unreachable_with_id(id, message, details);
}

/// Like [`unreachable`] but with an explicit assertion ID.
#[cfg(feature = "full")]
pub fn unreachable_with_id(id: u32, message: &str, details: &serde_json::Value) {
    let json_bytes = to_json_bytes(details);
    transport::hypercall(CMD_ASSERT_UNREACHABLE, 0x00, id, message, &json_bytes);
}

/// Assert `always` when true, `unreachable` when false.
#[cfg(feature = "full")]
pub fn always_or_unreachable(cond: bool, message: &str, details: &serde_json::Value) {
    let id = location_id(message);
    always_or_unreachable_with_id(cond, id, message, details);
}

/// Like [`always_or_unreachable`] but with an explicit assertion ID.
#[cfg(feature = "full")]
pub fn always_or_unreachable_with_id(
    cond: bool,
    id: u32,
    message: &str,
    details: &serde_json::Value,
) {
    if cond {
        always_with_id(cond, id, message, details);
    } else {
        unreachable_with_id(id, message, details);
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Core assertions (no-op mode)
// ═══════════════════════════════════════════════════════════════════════

#[cfg(not(feature = "full"))]
pub fn always(_cond: bool, _message: &str, _details: &()) {}
#[cfg(not(feature = "full"))]
pub fn always_with_id(_cond: bool, _id: u32, _message: &str, _details: &()) {}
#[cfg(not(feature = "full"))]
pub fn sometimes(_cond: bool, _message: &str, _details: &()) {}
#[cfg(not(feature = "full"))]
pub fn sometimes_with_id(_cond: bool, _id: u32, _message: &str, _details: &()) {}
#[cfg(not(feature = "full"))]
pub fn reachable(_message: &str, _details: &()) {}
#[cfg(not(feature = "full"))]
pub fn reachable_with_id(_id: u32, _message: &str, _details: &()) {}
#[cfg(not(feature = "full"))]
pub fn unreachable(_message: &str, _details: &()) {}
#[cfg(not(feature = "full"))]
pub fn unreachable_with_id(_id: u32, _message: &str, _details: &()) {}
#[cfg(not(feature = "full"))]
pub fn always_or_unreachable(_cond: bool, _message: &str, _details: &()) {}
#[cfg(not(feature = "full"))]
pub fn always_or_unreachable_with_id(_cond: bool, _id: u32, _message: &str, _details: &()) {}

// ═══════════════════════════════════════════════════════════════════════
//  Macros: empty JSON helper
// ═══════════════════════════════════════════════════════════════════════

/// Internal macro to produce empty JSON details.
#[cfg(feature = "full")]
#[doc(hidden)]
#[macro_export]
macro_rules! __cc_empty_json {
    () => {
        $crate::serde_json::json!({})
    };
}

#[cfg(not(feature = "full"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __cc_empty_json {
    () => {
        ()
    };
}

// ═══════════════════════════════════════════════════════════════════════
//  Basic assertion macros (auto location ID)
// ═══════════════════════════════════════════════════════════════════════

/// Assert-always with automatic source location ID.
///
/// Registers the assertion site in the compile-time catalog so the oracle
/// can detect whether the explorer ever exercises this code path.
///
/// ```rust,ignore
/// cc_assert_always!(leader_id < 3, "valid leader");
/// cc_assert_always!(leader_id < 3, "valid leader", &json!({"id": leader_id}));
/// ```
#[macro_export]
macro_rules! cc_assert_always {
    ($cond:expr, $msg:expr) => {{
        const _ID: u32 = $crate::assert::location_id(concat!(file!(), ":", line!(), ":", $msg));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        $crate::assert::always_with_id($cond, _ID, $msg, &$crate::__cc_empty_json!());
    }};
    ($cond:expr, $msg:expr, $details:expr) => {{
        const _ID: u32 = $crate::assert::location_id(concat!(file!(), ":", line!(), ":", $msg));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        $crate::assert::always_with_id($cond, _ID, $msg, $details);
    }};
}

/// Assert-sometimes with automatic source location ID.
#[macro_export]
macro_rules! cc_assert_sometimes {
    ($cond:expr, $msg:expr) => {{
        const _ID: u32 = $crate::assert::location_id(concat!(file!(), ":", line!(), ":", $msg));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        $crate::assert::sometimes_with_id($cond, _ID, $msg, &$crate::__cc_empty_json!());
    }};
    ($cond:expr, $msg:expr, $details:expr) => {{
        const _ID: u32 = $crate::assert::location_id(concat!(file!(), ":", line!(), ":", $msg));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        $crate::assert::sometimes_with_id($cond, _ID, $msg, $details);
    }};
}

/// Assert-reachable with automatic source location ID.
#[macro_export]
macro_rules! cc_assert_reachable {
    ($msg:expr) => {{
        const _ID: u32 = $crate::assert::location_id(concat!(file!(), ":", line!(), ":", $msg));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_REACHABLE);
        $crate::assert::reachable_with_id(_ID, $msg, &$crate::__cc_empty_json!());
    }};
    ($msg:expr, $details:expr) => {{
        const _ID: u32 = $crate::assert::location_id(concat!(file!(), ":", line!(), ":", $msg));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_REACHABLE);
        $crate::assert::reachable_with_id(_ID, $msg, $details);
    }};
}

/// Assert-unreachable with automatic source location ID.
#[macro_export]
macro_rules! cc_assert_unreachable {
    ($msg:expr) => {{
        const _ID: u32 = $crate::assert::location_id(concat!(file!(), ":", line!(), ":", $msg));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_UNREACHABLE);
        $crate::assert::unreachable_with_id(_ID, $msg, &$crate::__cc_empty_json!());
    }};
    ($msg:expr, $details:expr) => {{
        const _ID: u32 = $crate::assert::location_id(concat!(file!(), ":", line!(), ":", $msg));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_UNREACHABLE);
        $crate::assert::unreachable_with_id(_ID, $msg, $details);
    }};
}

/// Assert-always-or-unreachable with automatic source location ID.
#[macro_export]
macro_rules! cc_assert_always_or_unreachable {
    ($cond:expr, $msg:expr) => {{
        const _ID: u32 = $crate::assert::location_id(concat!(file!(), ":", line!(), ":", $msg));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        $crate::assert::always_or_unreachable_with_id(
            $cond,
            _ID,
            $msg,
            &$crate::__cc_empty_json!(),
        );
    }};
    ($cond:expr, $msg:expr, $details:expr) => {{
        const _ID: u32 = $crate::assert::location_id(concat!(file!(), ":", line!(), ":", $msg));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        $crate::assert::always_or_unreachable_with_id($cond, _ID, $msg, $details);
    }};
}

// ═══════════════════════════════════════════════════════════════════════
//  Catalog registration helper macro
// ═══════════════════════════════════════════════════════════════════════

/// Internal macro to register an assertion site in the compile-time catalog.
///
/// When `full` is enabled, emits a static `CatalogEntry` into the
/// `ASSERTION_CATALOG` distributed slice.  No-op without `full`.
#[cfg(feature = "full")]
#[doc(hidden)]
#[macro_export]
macro_rules! __cc_register_catalog {
    ($id:expr, $msg:expr, $kind:expr) => {
        const _: () = {
            #[linkme::distributed_slice($crate::assert::ASSERTION_CATALOG)]
            static _CATALOG_ENTRY: $crate::assert::CatalogEntry = $crate::assert::CatalogEntry {
                id: $crate::assert::location_id(concat!(file!(), ":", line!(), ":", $msg)),
                message: $msg,
                kind: $kind,
                file: file!(),
                line: line!(),
            };
        };
    };
}

#[cfg(not(feature = "full"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __cc_register_catalog {
    ($id:expr, $msg:expr, $kind:expr) => {};
}

// ═══════════════════════════════════════════════════════════════════════
//  Numeric comparison macros
// ═══════════════════════════════════════════════════════════════════════

/// Assert `left < right` always holds.
#[macro_export]
macro_rules! cc_assert_always_lt {
    ($left:expr, $right:expr, $msg:expr) => {{
        const _ID: u32 = $crate::assert::location_id(concat!(file!(), ":", line!(), ":", $msg));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        $crate::assert::always_with_id($left < $right, _ID, $msg, &$crate::__cc_empty_json!());
    }};
}

/// Assert `left <= right` always holds.
#[macro_export]
macro_rules! cc_assert_always_le {
    ($left:expr, $right:expr, $msg:expr) => {{
        const _ID: u32 = $crate::assert::location_id(concat!(file!(), ":", line!(), ":", $msg));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        $crate::assert::always_with_id($left <= $right, _ID, $msg, &$crate::__cc_empty_json!());
    }};
}

/// Assert `left > right` always holds.
#[macro_export]
macro_rules! cc_assert_always_gt {
    ($left:expr, $right:expr, $msg:expr) => {{
        const _ID: u32 = $crate::assert::location_id(concat!(file!(), ":", line!(), ":", $msg));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        $crate::assert::always_with_id($left > $right, _ID, $msg, &$crate::__cc_empty_json!());
    }};
}

/// Assert `left >= right` always holds.
#[macro_export]
macro_rules! cc_assert_always_ge {
    ($left:expr, $right:expr, $msg:expr) => {{
        const _ID: u32 = $crate::assert::location_id(concat!(file!(), ":", line!(), ":", $msg));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        $crate::assert::always_with_id($left >= $right, _ID, $msg, &$crate::__cc_empty_json!());
    }};
}

/// Assert `left == right` always holds.
#[macro_export]
macro_rules! cc_assert_always_eq {
    ($left:expr, $right:expr, $msg:expr) => {{
        const _ID: u32 = $crate::assert::location_id(concat!(file!(), ":", line!(), ":", $msg));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        $crate::assert::always_with_id($left == $right, _ID, $msg, &$crate::__cc_empty_json!());
    }};
}

/// Assert `left != right` always holds.
#[macro_export]
macro_rules! cc_assert_always_ne {
    ($left:expr, $right:expr, $msg:expr) => {{
        const _ID: u32 = $crate::assert::location_id(concat!(file!(), ":", line!(), ":", $msg));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        $crate::assert::always_with_id($left != $right, _ID, $msg, &$crate::__cc_empty_json!());
    }};
}

/// Assert `left < right` sometimes holds.
#[macro_export]
macro_rules! cc_assert_sometimes_lt {
    ($left:expr, $right:expr, $msg:expr) => {{
        const _ID: u32 = $crate::assert::location_id(concat!(file!(), ":", line!(), ":", $msg));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        $crate::assert::sometimes_with_id($left < $right, _ID, $msg, &$crate::__cc_empty_json!());
    }};
}

/// Assert `left <= right` sometimes holds.
#[macro_export]
macro_rules! cc_assert_sometimes_le {
    ($left:expr, $right:expr, $msg:expr) => {{
        const _ID: u32 = $crate::assert::location_id(concat!(file!(), ":", line!(), ":", $msg));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        $crate::assert::sometimes_with_id($left <= $right, _ID, $msg, &$crate::__cc_empty_json!());
    }};
}

/// Assert `left > right` sometimes holds.
#[macro_export]
macro_rules! cc_assert_sometimes_gt {
    ($left:expr, $right:expr, $msg:expr) => {{
        const _ID: u32 = $crate::assert::location_id(concat!(file!(), ":", line!(), ":", $msg));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        $crate::assert::sometimes_with_id($left > $right, _ID, $msg, &$crate::__cc_empty_json!());
    }};
}

/// Assert `left >= right` sometimes holds.
#[macro_export]
macro_rules! cc_assert_sometimes_ge {
    ($left:expr, $right:expr, $msg:expr) => {{
        const _ID: u32 = $crate::assert::location_id(concat!(file!(), ":", line!(), ":", $msg));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        $crate::assert::sometimes_with_id($left >= $right, _ID, $msg, &$crate::__cc_empty_json!());
    }};
}

/// Assert `left == right` sometimes holds.
#[macro_export]
macro_rules! cc_assert_sometimes_eq {
    ($left:expr, $right:expr, $msg:expr) => {{
        const _ID: u32 = $crate::assert::location_id(concat!(file!(), ":", line!(), ":", $msg));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        $crate::assert::sometimes_with_id($left == $right, _ID, $msg, &$crate::__cc_empty_json!());
    }};
}

/// Assert `left != right` sometimes holds.
#[macro_export]
macro_rules! cc_assert_sometimes_ne {
    ($left:expr, $right:expr, $msg:expr) => {{
        const _ID: u32 = $crate::assert::location_id(concat!(file!(), ":", line!(), ":", $msg));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        $crate::assert::sometimes_with_id($left != $right, _ID, $msg, &$crate::__cc_empty_json!());
    }};
}

/// Assert that an option is `Some` every time.
#[macro_export]
macro_rules! cc_assert_always_some {
    ($expr:expr, $msg:expr) => {{
        const _ID: u32 = $crate::assert::location_id(concat!(file!(), ":", line!(), ":", $msg));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        $crate::assert::always_with_id($expr.is_some(), _ID, $msg, &$crate::__cc_empty_json!());
    }};
}

/// Assert that an option is `Some` at least once across runs.
#[macro_export]
macro_rules! cc_assert_sometimes_some {
    ($expr:expr, $msg:expr) => {{
        const _ID: u32 = $crate::assert::location_id(concat!(file!(), ":", line!(), ":", $msg));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        $crate::assert::sometimes_with_id($expr.is_some(), _ID, $msg, &$crate::__cc_empty_json!());
    }};
}

// serde_json re-exported from lib.rs for macro use via $crate::serde_json

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
        assert_eq!(id, 0x811c_9dc5);
    }

    #[test]
    fn location_id_single_char_difference() {
        let a = location_id("a");
        let b = location_id("b");
        assert_ne!(a, b);
    }

    #[test]
    fn basic_macros_compile() {
        cc_assert_always!(true, "test always");
        cc_assert_sometimes!(true, "test sometimes");
        cc_assert_reachable!("test reachable");
        cc_assert_always_or_unreachable!(true, "test always_or_unreachable");
    }

    #[test]
    fn macros_with_json_details() {
        use serde_json::json;
        cc_assert_always!(true, "msg", &json!({"key": 42}));
        cc_assert_sometimes!(true, "msg2", &json!({"x": "y"}));
        cc_assert_reachable!("msg3", &json!({}));
    }

    #[test]
    fn comparison_macros_compile() {
        let a = 5;
        let b = 10;

        cc_assert_always_lt!(a, b, "a < b");
        cc_assert_always_le!(a, b, "a <= b");
        cc_assert_always_gt!(b, a, "b > a");
        cc_assert_always_ge!(b, a, "b >= a");
        cc_assert_always_eq!(a, a, "a == a");
        cc_assert_always_ne!(a, b, "a != b");

        cc_assert_sometimes_lt!(a, b, "sometimes a < b");
        cc_assert_sometimes_le!(a, b, "sometimes a <= b");
        cc_assert_sometimes_gt!(b, a, "sometimes b > a");
        cc_assert_sometimes_ge!(b, a, "sometimes b >= a");
        cc_assert_sometimes_eq!(a, a, "sometimes a == a");
        cc_assert_sometimes_ne!(a, b, "sometimes a != b");
    }

    #[test]
    fn option_macros_compile() {
        cc_assert_always_some!(Some(42), "has value");
        cc_assert_sometimes_some!(Some("x"), "sometimes has value");
    }

    #[test]
    fn always_or_unreachable_when_true() {
        use serde_json::json;
        always_or_unreachable(true, "test", &json!({"key": "val"}));
    }
}
