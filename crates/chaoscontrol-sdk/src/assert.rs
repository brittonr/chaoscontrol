//! Test property assertions — the core of ChaosControl's testing model.
//!
//! These assertions communicate properties to the VMM, which tracks them
//! across thousands of deterministic runs to find violations.
//!
//! # Assertion semantics
//!
//! | Macro | Single-run behavior | Cross-run aggregation |
//! |-------|---------------------|-----------------------|
//! | `cc_assert_always!` | Fail if `cond` is false | Fail after any false result |
//! | `cc_assert_sometimes!` | Record `cond` | Pass after any true result |
//! | `cc_assert_reachable!` | Record a hit | Pass after a hit |
//! | `cc_assert_unreachable!` | Fail immediately | Fail after any hit |
//!
//! # Assertion identity
//!
//! Assertion macros register automatic structured descriptors. Stable macros
//! require an explicit namespace and key. The compact `u32` field is only a
//! transport alias. It is not unique, authoritative, or valid without the
//! accepted catalog token and descriptor fingerprint.

#[cfg(feature = "full")]
use crate::transport;
use chaoscontrol_protocol::*;

/// Re-export the details module for `assert::details::*` access.
#[cfg(feature = "full")]
pub use crate::details;

// ═══════════════════════════════════════════════════════════════════════
//  Assertion catalog (compile-time registration via linkme)
// ═══════════════════════════════════════════════════════════════════════

/// Catalog kind constants for `CatalogEntry::kind`.
pub const CATALOG_KIND_ALWAYS: u8 = chaoscontrol_protocol::ASSERTION_KIND_ALWAYS_DISCRIMINANT;
pub const CATALOG_KIND_SOMETIMES: u8 = chaoscontrol_protocol::ASSERTION_KIND_SOMETIMES_DISCRIMINANT;
pub const CATALOG_KIND_REACHABLE: u8 = chaoscontrol_protocol::ASSERTION_KIND_REACHABLE_DISCRIMINANT;
pub const CATALOG_KIND_UNREACHABLE: u8 =
    chaoscontrol_protocol::ASSERTION_KIND_UNREACHABLE_DISCRIMINANT;

/// The kind of assertion being made.
///
/// Maps to the four assertion semantics supported by ChaosControl.
/// Used with `cc_assert_raw!` and stable structured assertion APIs.
///
/// # Third-party framework integration
///
/// ```rust,ignore
/// use chaoscontrol_sdk::prelude::*;
///
/// // Route a proptest property through a registered assertion site.
/// let result = my_proptest_check();
/// cc_assert_raw!(
///     AssertionKind::Always,
///     result.is_ok(),
///     "proptest property holds",
///     &json!({"error": format!("{:?}", result)}),
/// );
/// ```
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AssertionKind {
    /// Must be true every time the assertion is reached.
    Always,
    /// Must be true at least once across all runs.
    Sometimes,
    /// This code point must be reached at least once across all runs.
    Reachable,
    /// This code point must never be reached.
    Unreachable,
}

impl AssertionKind {
    /// Map to the corresponding `CATALOG_KIND_*` constant.
    ///
    /// This is `const` so it can be used in static catalog entries
    /// (e.g. inside `cc_assert_raw!` macro expansion).
    pub const fn to_catalog_kind(&self) -> u8 {
        match self {
            AssertionKind::Always => CATALOG_KIND_ALWAYS,
            AssertionKind::Sometimes => CATALOG_KIND_SOMETIMES,
            AssertionKind::Reachable => CATALOG_KIND_REACHABLE,
            AssertionKind::Unreachable => CATALOG_KIND_UNREACHABLE,
        }
    }

    /// Map to the corresponding `CMD_ASSERT_*` protocol command.
    pub fn to_command(&self) -> u8 {
        match self {
            AssertionKind::Always => CMD_ASSERT_ALWAYS,
            AssertionKind::Sometimes => CMD_ASSERT_SOMETIMES,
            AssertionKind::Reachable => CMD_ASSERT_REACHABLE,
            AssertionKind::Unreachable => CMD_ASSERT_UNREACHABLE,
        }
    }
}

/// A registered assertion site, created at link time by assertion macros.
///
/// Every `cc_assert_*!` macro emits a static `CatalogEntry` into the
/// `ASSERTION_CATALOG` distributed slice.  At initialization, the SDK
/// sends all catalog entries to the VMM so the oracle can distinguish
/// "never hit" from "never registered."
#[derive(Clone, Copy)]
pub enum CatalogLogicalKey {
    Automatic(&'static str),
    Stable(&'static str),
}

pub struct CatalogEntry {
    /// Non-authoritative compact transport alias.
    pub id: u32,
    /// Versioned catalog namespace.
    pub namespace: &'static str,
    /// Authoritative logical key.
    pub logical_key: CatalogLogicalKey,
    /// Human-readable assertion message.
    pub message: &'static str,
    /// Kind of assertion: one of `CATALOG_KIND_*` constants.
    pub kind: u8,
    /// Source file where the assertion macro was invoked.
    pub file: &'static str,
    /// Source line where the assertion macro was invoked.
    pub line: u32,
    /// Source column where the assertion macro was invoked.
    pub column: u32,
    /// Guest name for density reporting.
    pub guest: &'static str,
    /// Normalized bounded metadata label, such as `invariant`,
    /// `workload-driver`, or `service-invariant`.
    pub category: &'static str,
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
/// strict catalog transport so the oracle knows about every
/// assertion site — including ones that are never reached.
#[cfg(feature = "full")]
pub(crate) fn emit_catalog() {
    crate::assertion_catalog::emit_catalog();
}

// ═══════════════════════════════════════════════════════════════════════
//  Location ID
// ═══════════════════════════════════════════════════════════════════════

const ASSERTION_ALIAS_FNV1A_OFFSET_BASIS: u32 = 0x811c_9dc5;
const ASSERTION_ALIAS_FNV1A_PRIME: u32 = 0x0100_0193;

/// Derive a non-authoritative compact transport alias.
///
/// The accepted catalog token and descriptor fingerprint remain authoritative.
#[doc(hidden)]
pub const fn location_id(location: &str) -> u32 {
    let bytes = location.as_bytes();
    let mut hash = ASSERTION_ALIAS_FNV1A_OFFSET_BASIS;
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u32;
        hash = hash.wrapping_mul(ASSERTION_ALIAS_FNV1A_PRIME);
        i += 1;
    }
    hash
}

// ═══════════════════════════════════════════════════════════════════════
//  JSON serialization helper
// ═══════════════════════════════════════════════════════════════════════

/// Serialize details to JSON bytes. Only available with `full` feature.
#[cfg(feature = "full")]
fn to_json_bytes(details: &serde_json::Value) -> Option<Vec<u8>> {
    crate::bounded_json::assertion_details(details).ok()
}

// ═══════════════════════════════════════════════════════════════════════
//  Core assertions (full mode)
// ═══════════════════════════════════════════════════════════════════════

#[cfg(feature = "full")]
#[doc(hidden)]
pub fn emit_registered_assertion(
    kind: AssertionKind,
    cond: bool,
    id: u32,
    message: &str,
    details: &serde_json::Value,
) {
    emit_registered_assertion_core(kind, cond, id, message, details);
}

#[cfg(feature = "full")]
#[doc(hidden)]
pub fn assert_raw_with_key(
    kind: AssertionKind,
    cond: bool,
    namespace: &str,
    key: &str,
    message: &str,
    details: &serde_json::Value,
) {
    let flags = assertion_flags(kind, cond);
    let Some(json_bytes) = to_json_bytes(details) else {
        transport::hypercall_raw(kind.to_command(), flags, 0, &[]);
        return;
    };
    let Some(identity) = crate::assertion_catalog::resolve_stable(namespace, key, kind, message)
    else {
        transport::hypercall_raw(kind.to_command(), flags, 0, &[]);
        return;
    };
    transport::hypercall_bound_assertion(
        kind.to_command(),
        flags,
        0,
        message,
        &json_bytes,
        identity,
    );
}

#[cfg(feature = "full")]
fn emit_registered_assertion_core(
    kind: AssertionKind,
    cond: bool,
    id: u32,
    message: &str,
    details: &serde_json::Value,
) {
    let flags = assertion_flags(kind, cond);
    let Some(json_bytes) = to_json_bytes(details) else {
        transport::hypercall_raw(kind.to_command(), flags, id, &[]);
        return;
    };
    let Some(identity) = crate::assertion_catalog::resolve_compatibility(id, kind, message) else {
        transport::hypercall_raw(kind.to_command(), flags, id, &[]);
        return;
    };
    transport::hypercall_bound_assertion(
        kind.to_command(),
        flags,
        id,
        message,
        &json_bytes,
        identity,
    );
}

#[cfg(feature = "full")]
fn assertion_flags(kind: AssertionKind, condition: bool) -> u8 {
    match kind {
        AssertionKind::Always | AssertionKind::Sometimes => u8::from(condition),
        AssertionKind::Reachable => 1,
        AssertionKind::Unreachable => 0,
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Core assertions (no-op mode)
// ═══════════════════════════════════════════════════════════════════════

#[cfg(not(feature = "full"))]
#[doc(hidden)]
pub fn emit_registered_assertion(
    _kind: AssertionKind,
    _cond: bool,
    _id: u32,
    _message: &str,
    _details: &(),
) {
}
#[cfg(not(feature = "full"))]
#[doc(hidden)]
pub fn assert_raw_with_key(
    _kind: AssertionKind,
    _cond: bool,
    _namespace: &str,
    _key: &str,
    _message: &str,
    _details: &(),
) {
}

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
    ($cond:expr, $msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Always,
            $cond,
            _ID,
            $msg,
            &$crate::__cc_empty_json!(),
        );
    }};
    ($cond:expr, $msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Always,
            $cond,
            _ID,
            $msg,
            $details,
        );
    }};
}

/// Assert-sometimes with automatic source location ID.
#[macro_export]
macro_rules! cc_assert_sometimes {
    ($cond:expr, $msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Sometimes,
            $cond,
            _ID,
            $msg,
            &$crate::__cc_empty_json!(),
        );
    }};
    ($cond:expr, $msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Sometimes,
            $cond,
            _ID,
            $msg,
            $details,
        );
    }};
}

/// Assert-reachable with automatic source location ID.
#[macro_export]
macro_rules! cc_assert_reachable {
    ($msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_REACHABLE);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Reachable,
            true,
            _ID,
            $msg,
            &$crate::__cc_empty_json!(),
        );
    }};
    ($msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_REACHABLE);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Reachable,
            true,
            _ID,
            $msg,
            $details,
        );
    }};
}

/// Assert-unreachable with automatic source location ID.
#[macro_export]
macro_rules! cc_assert_unreachable {
    ($msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_UNREACHABLE);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Unreachable,
            false,
            _ID,
            $msg,
            &$crate::__cc_empty_json!(),
        );
    }};
    ($msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_UNREACHABLE);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Unreachable,
            false,
            _ID,
            $msg,
            $details,
        );
    }};
}

/// Assert-always-or-unreachable with automatic source location ID.
#[macro_export]
macro_rules! cc_assert_always_or_unreachable {
    ($cond:expr, $msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Always,
            $cond,
            _ID,
            $msg,
            &$crate::__cc_empty_json!(),
        );
    }};
    ($cond:expr, $msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Always,
            $cond,
            _ID,
            $msg,
            $details,
        );
    }};
}

/// Generic assertion macro with catalog registration.
///
/// Like [`cc_assert_always!`] and friends, but takes the assertion kind
/// as a parameter. Registers the assertion site in the compile-time
/// catalog and dispatches through the internal bound-event path at runtime.
///
/// The `kind` argument must be a const-evaluable expression, such as
/// `AssertionKind::Always`. Runtime-computed assertion kinds are unsupported.
///
/// ```rust,ignore
/// use chaoscontrol_sdk::prelude::*;
///
/// // Equivalent to cc_assert_always!(cond, "msg"):
/// cc_assert_raw!(AssertionKind::Always, cond, "msg");
///
/// // With details:
/// cc_assert_raw!(AssertionKind::Sometimes, cond, "msg", &json!({"k": "v"}));
/// ```
#[macro_export]
macro_rules! cc_assert_raw {
    ($kind:expr, $cond:expr, $msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $kind.to_catalog_kind());
        $crate::assert::emit_registered_assertion(
            $kind,
            $cond,
            _ID,
            $msg,
            &$crate::__cc_empty_json!(),
        );
    }};
    ($kind:expr, $cond:expr, $msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $kind.to_catalog_kind());
        $crate::assert::emit_registered_assertion($kind, $cond, _ID, $msg, $details);
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
    ($id:expr, $msg:expr, $kind:expr, $guest:expr, $category:expr) => {
        const _: () = {
            #[linkme::distributed_slice($crate::assert::ASSERTION_CATALOG)]
            static _CATALOG_ENTRY: $crate::assert::CatalogEntry = $crate::assert::CatalogEntry {
                id: $id,
                namespace: concat!(
                    "build:",
                    env!("CARGO_PKG_NAME"),
                    ":",
                    env!("CARGO_PKG_VERSION")
                ),
                logical_key: $crate::assert::CatalogLogicalKey::Automatic(concat!(
                    file!(),
                    ":",
                    line!(),
                    ":",
                    column!()
                )),
                message: $msg,
                kind: $kind,
                file: file!(),
                line: line!(),
                column: column!(),
                guest: $guest,
                category: $category,
            };
        };
    };
    ($id:expr, $msg:expr, $kind:expr) => {
        $crate::__cc_register_catalog!($id, $msg, $kind, "uncategorized", "uncategorized");
    };
}

#[cfg(feature = "full")]
#[doc(hidden)]
#[macro_export]
macro_rules! __cc_register_stable_catalog {
    ($namespace:expr, $key:expr, $id:expr, $msg:expr, $kind:expr, $guest:expr, $category:expr) => {
        const _: () = {
            #[linkme::distributed_slice($crate::assert::ASSERTION_CATALOG)]
            static _CATALOG_ENTRY: $crate::assert::CatalogEntry = $crate::assert::CatalogEntry {
                id: $id,
                namespace: $namespace,
                logical_key: $crate::assert::CatalogLogicalKey::Stable($key),
                message: $msg,
                kind: $kind,
                file: file!(),
                line: line!(),
                column: column!(),
                guest: $guest,
                category: $category,
            };
        };
    };
}

#[cfg(not(feature = "full"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __cc_register_catalog {
    ($id:expr, $msg:expr, $kind:expr) => {};
    ($id:expr, $msg:expr, $kind:expr, $guest:expr, $category:expr) => {};
}

#[cfg(not(feature = "full"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __cc_register_stable_catalog {
    ($namespace:expr, $key:expr, $id:expr, $msg:expr, $kind:expr, $guest:expr, $category:expr) => {};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __cc_assert_stable {
    ($kind:expr, $catalog_kind:expr, $namespace:expr, $key:expr, $guest:expr, $category:expr, $cond:expr, $msg:expr, $details:expr) => {{
        const _ID: u32 = $crate::assert::location_id(concat!($namespace, ":", $key));
        $crate::__cc_register_stable_catalog!(
            $namespace,
            $key,
            _ID,
            $msg,
            $catalog_kind,
            $guest,
            $category
        );
        $crate::assert::assert_raw_with_key($kind, $cond, $namespace, $key, $msg, $details);
    }};
}

#[macro_export]
macro_rules! cc_assert_always_stable {
    ($namespace:expr, $key:expr, $guest:expr, $category:expr, $cond:expr, $msg:expr $(,)?) => {{
        let __cc_details = $crate::__cc_empty_json!();
        $crate::__cc_assert_stable!(
            $crate::assert::AssertionKind::Always,
            $crate::assert::CATALOG_KIND_ALWAYS,
            $namespace,
            $key,
            $guest,
            $category,
            $cond,
            $msg,
            &__cc_details
        );
    }};
    ($namespace:expr, $key:expr, $guest:expr, $category:expr, $cond:expr, $msg:expr, $details:expr $(,)?) => {{
        $crate::__cc_assert_stable!(
            $crate::assert::AssertionKind::Always,
            $crate::assert::CATALOG_KIND_ALWAYS,
            $namespace,
            $key,
            $guest,
            $category,
            $cond,
            $msg,
            $details
        );
    }};
}

#[macro_export]
macro_rules! cc_assert_sometimes_stable {
    ($namespace:expr, $key:expr, $guest:expr, $category:expr, $cond:expr, $msg:expr $(,)?) => {{
        let __cc_details = $crate::__cc_empty_json!();
        $crate::__cc_assert_stable!(
            $crate::assert::AssertionKind::Sometimes,
            $crate::assert::CATALOG_KIND_SOMETIMES,
            $namespace,
            $key,
            $guest,
            $category,
            $cond,
            $msg,
            &__cc_details
        );
    }};
    ($namespace:expr, $key:expr, $guest:expr, $category:expr, $cond:expr, $msg:expr, $details:expr $(,)?) => {{
        $crate::__cc_assert_stable!(
            $crate::assert::AssertionKind::Sometimes,
            $crate::assert::CATALOG_KIND_SOMETIMES,
            $namespace,
            $key,
            $guest,
            $category,
            $cond,
            $msg,
            $details
        );
    }};
}

#[macro_export]
macro_rules! cc_assert_reachable_stable {
    ($namespace:expr, $key:expr, $guest:expr, $category:expr, $msg:expr $(,)?) => {{
        let __cc_details = $crate::__cc_empty_json!();
        $crate::__cc_assert_stable!(
            $crate::assert::AssertionKind::Reachable,
            $crate::assert::CATALOG_KIND_REACHABLE,
            $namespace,
            $key,
            $guest,
            $category,
            true,
            $msg,
            &__cc_details
        );
    }};
    ($namespace:expr, $key:expr, $guest:expr, $category:expr, $msg:expr, $details:expr $(,)?) => {{
        $crate::__cc_assert_stable!(
            $crate::assert::AssertionKind::Reachable,
            $crate::assert::CATALOG_KIND_REACHABLE,
            $namespace,
            $key,
            $guest,
            $category,
            true,
            $msg,
            $details
        );
    }};
}

#[macro_export]
macro_rules! cc_assert_unreachable_stable {
    ($namespace:expr, $key:expr, $guest:expr, $category:expr, $msg:expr $(,)?) => {{
        let __cc_details = $crate::__cc_empty_json!();
        $crate::__cc_assert_stable!(
            $crate::assert::AssertionKind::Unreachable,
            $crate::assert::CATALOG_KIND_UNREACHABLE,
            $namespace,
            $key,
            $guest,
            $category,
            false,
            $msg,
            &__cc_details
        );
    }};
    ($namespace:expr, $key:expr, $guest:expr, $category:expr, $msg:expr, $details:expr $(,)?) => {{
        $crate::__cc_assert_stable!(
            $crate::assert::AssertionKind::Unreachable,
            $crate::assert::CATALOG_KIND_UNREACHABLE,
            $namespace,
            $key,
            $guest,
            $category,
            false,
            $msg,
            $details
        );
    }};
}

/// Categorized assert-always with automatic source location ID.
#[macro_export]
macro_rules! cc_assert_always_category {
    ($guest:expr, $category:expr, $cond:expr, $msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(
            _ID,
            $msg,
            $crate::assert::CATALOG_KIND_ALWAYS,
            $guest,
            $category
        );
        let __cc_details = $crate::__cc_empty_json!();
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Always,
            $cond,
            _ID,
            $msg,
            &__cc_details,
        );
    }};
    ($guest:expr, $category:expr, $cond:expr, $msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(
            _ID,
            $msg,
            $crate::assert::CATALOG_KIND_ALWAYS,
            $guest,
            $category
        );
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Always,
            $cond,
            _ID,
            $msg,
            $details,
        );
    }};
}

/// Categorized assert-sometimes with automatic source location ID.
#[macro_export]
macro_rules! cc_assert_sometimes_category {
    ($guest:expr, $category:expr, $cond:expr, $msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(
            _ID,
            $msg,
            $crate::assert::CATALOG_KIND_SOMETIMES,
            $guest,
            $category
        );
        let __cc_details = $crate::__cc_empty_json!();
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Sometimes,
            $cond,
            _ID,
            $msg,
            &__cc_details,
        );
    }};
    ($guest:expr, $category:expr, $cond:expr, $msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(
            _ID,
            $msg,
            $crate::assert::CATALOG_KIND_SOMETIMES,
            $guest,
            $category
        );
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Sometimes,
            $cond,
            _ID,
            $msg,
            $details,
        );
    }};
}

/// Categorized assert-reachable with automatic source location ID.
#[macro_export]
macro_rules! cc_assert_reachable_category {
    ($guest:expr, $category:expr, $msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(
            _ID,
            $msg,
            $crate::assert::CATALOG_KIND_REACHABLE,
            $guest,
            $category
        );
        let __cc_details = $crate::__cc_empty_json!();
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Reachable,
            true,
            _ID,
            $msg,
            &__cc_details,
        );
    }};
    ($guest:expr, $category:expr, $msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(
            _ID,
            $msg,
            $crate::assert::CATALOG_KIND_REACHABLE,
            $guest,
            $category
        );
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Reachable,
            true,
            _ID,
            $msg,
            $details,
        );
    }};
}

// ═══════════════════════════════════════════════════════════════════════
//  Comparison detail helper (full vs no-op)
// ═══════════════════════════════════════════════════════════════════════

/// Build comparison details on failure: `{"left": "...", "right": "..."}`, empty `{}` on success.
#[cfg(feature = "full")]
#[doc(hidden)]
#[macro_export]
macro_rules! __cc_cmp_details {
    ($cond:expr, $left:ident, $right:ident) => {
        if $cond {
            $crate::serde_json::json!({})
        } else {
            $crate::serde_json::json!({
                "left": format!("{:?}", $left),
                "right": format!("{:?}", $right),
            })
        }
    };
}

#[cfg(not(feature = "full"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __cc_cmp_details {
    ($cond:expr, $left:ident, $right:ident) => {
        ()
    };
}

/// Build option details on failure: `{"value": "None"}`, empty `{}` on success.
#[cfg(feature = "full")]
#[doc(hidden)]
#[macro_export]
macro_rules! __cc_option_details {
    ($cond:expr) => {
        if $cond {
            $crate::serde_json::json!({})
        } else {
            $crate::serde_json::json!({"value": "None"})
        }
    };
}

#[cfg(not(feature = "full"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __cc_option_details {
    ($cond:expr) => {
        ()
    };
}

// ═══════════════════════════════════════════════════════════════════════
//  Numeric comparison macros
// ═══════════════════════════════════════════════════════════════════════

/// Assert `left < right` always holds. Auto-captures left/right on failure.
#[macro_export]
macro_rules! cc_assert_always_lt {
    ($left:expr, $right:expr, $msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        let __cc_left = $left;
        let __cc_right = $right;
        let __cc_cond = __cc_left < __cc_right;
        let __cc_det = $crate::__cc_cmp_details!(__cc_cond, __cc_left, __cc_right);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Always,
            __cc_cond,
            _ID,
            $msg,
            &__cc_det,
        );
    }};
    ($left:expr, $right:expr, $msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Always,
            $left < $right,
            _ID,
            $msg,
            $details,
        );
    }};
}

/// Assert `left <= right` always holds. Auto-captures left/right on failure.
#[macro_export]
macro_rules! cc_assert_always_le {
    ($left:expr, $right:expr, $msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        let __cc_left = $left;
        let __cc_right = $right;
        let __cc_cond = __cc_left <= __cc_right;
        let __cc_det = $crate::__cc_cmp_details!(__cc_cond, __cc_left, __cc_right);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Always,
            __cc_cond,
            _ID,
            $msg,
            &__cc_det,
        );
    }};
    ($left:expr, $right:expr, $msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Always,
            $left <= $right,
            _ID,
            $msg,
            $details,
        );
    }};
}

/// Assert `left > right` always holds. Auto-captures left/right on failure.
#[macro_export]
macro_rules! cc_assert_always_gt {
    ($left:expr, $right:expr, $msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        let __cc_left = $left;
        let __cc_right = $right;
        let __cc_cond = __cc_left > __cc_right;
        let __cc_det = $crate::__cc_cmp_details!(__cc_cond, __cc_left, __cc_right);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Always,
            __cc_cond,
            _ID,
            $msg,
            &__cc_det,
        );
    }};
    ($left:expr, $right:expr, $msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Always,
            $left > $right,
            _ID,
            $msg,
            $details,
        );
    }};
}

/// Assert `left >= right` always holds. Auto-captures left/right on failure.
#[macro_export]
macro_rules! cc_assert_always_ge {
    ($left:expr, $right:expr, $msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        let __cc_left = $left;
        let __cc_right = $right;
        let __cc_cond = __cc_left >= __cc_right;
        let __cc_det = $crate::__cc_cmp_details!(__cc_cond, __cc_left, __cc_right);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Always,
            __cc_cond,
            _ID,
            $msg,
            &__cc_det,
        );
    }};
    ($left:expr, $right:expr, $msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Always,
            $left >= $right,
            _ID,
            $msg,
            $details,
        );
    }};
}

/// Assert `left == right` always holds. Auto-captures left/right on failure.
#[macro_export]
macro_rules! cc_assert_always_eq {
    ($left:expr, $right:expr, $msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        let __cc_left = $left;
        let __cc_right = $right;
        let __cc_cond = __cc_left == __cc_right;
        let __cc_det = $crate::__cc_cmp_details!(__cc_cond, __cc_left, __cc_right);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Always,
            __cc_cond,
            _ID,
            $msg,
            &__cc_det,
        );
    }};
    ($left:expr, $right:expr, $msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Always,
            $left == $right,
            _ID,
            $msg,
            $details,
        );
    }};
}

/// Assert `left != right` always holds. Auto-captures left/right on failure.
#[macro_export]
macro_rules! cc_assert_always_ne {
    ($left:expr, $right:expr, $msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        let __cc_left = $left;
        let __cc_right = $right;
        let __cc_cond = __cc_left != __cc_right;
        let __cc_det = $crate::__cc_cmp_details!(__cc_cond, __cc_left, __cc_right);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Always,
            __cc_cond,
            _ID,
            $msg,
            &__cc_det,
        );
    }};
    ($left:expr, $right:expr, $msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Always,
            $left != $right,
            _ID,
            $msg,
            $details,
        );
    }};
}

/// Assert `left < right` sometimes holds. Auto-captures left/right on failure.
#[macro_export]
macro_rules! cc_assert_sometimes_lt {
    ($left:expr, $right:expr, $msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        let __cc_left = $left;
        let __cc_right = $right;
        let __cc_cond = __cc_left < __cc_right;
        let __cc_det = $crate::__cc_cmp_details!(__cc_cond, __cc_left, __cc_right);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Sometimes,
            __cc_cond,
            _ID,
            $msg,
            &__cc_det,
        );
    }};
    ($left:expr, $right:expr, $msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Sometimes,
            $left < $right,
            _ID,
            $msg,
            $details,
        );
    }};
}

/// Assert `left <= right` sometimes holds. Auto-captures left/right on failure.
#[macro_export]
macro_rules! cc_assert_sometimes_le {
    ($left:expr, $right:expr, $msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        let __cc_left = $left;
        let __cc_right = $right;
        let __cc_cond = __cc_left <= __cc_right;
        let __cc_det = $crate::__cc_cmp_details!(__cc_cond, __cc_left, __cc_right);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Sometimes,
            __cc_cond,
            _ID,
            $msg,
            &__cc_det,
        );
    }};
    ($left:expr, $right:expr, $msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Sometimes,
            $left <= $right,
            _ID,
            $msg,
            $details,
        );
    }};
}

/// Assert `left > right` sometimes holds. Auto-captures left/right on failure.
#[macro_export]
macro_rules! cc_assert_sometimes_gt {
    ($left:expr, $right:expr, $msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        let __cc_left = $left;
        let __cc_right = $right;
        let __cc_cond = __cc_left > __cc_right;
        let __cc_det = $crate::__cc_cmp_details!(__cc_cond, __cc_left, __cc_right);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Sometimes,
            __cc_cond,
            _ID,
            $msg,
            &__cc_det,
        );
    }};
    ($left:expr, $right:expr, $msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Sometimes,
            $left > $right,
            _ID,
            $msg,
            $details,
        );
    }};
}

/// Assert `left >= right` sometimes holds. Auto-captures left/right on failure.
#[macro_export]
macro_rules! cc_assert_sometimes_ge {
    ($left:expr, $right:expr, $msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        let __cc_left = $left;
        let __cc_right = $right;
        let __cc_cond = __cc_left >= __cc_right;
        let __cc_det = $crate::__cc_cmp_details!(__cc_cond, __cc_left, __cc_right);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Sometimes,
            __cc_cond,
            _ID,
            $msg,
            &__cc_det,
        );
    }};
    ($left:expr, $right:expr, $msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Sometimes,
            $left >= $right,
            _ID,
            $msg,
            $details,
        );
    }};
}

/// Assert `left == right` sometimes holds. Auto-captures left/right on failure.
#[macro_export]
macro_rules! cc_assert_sometimes_eq {
    ($left:expr, $right:expr, $msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        let __cc_left = $left;
        let __cc_right = $right;
        let __cc_cond = __cc_left == __cc_right;
        let __cc_det = $crate::__cc_cmp_details!(__cc_cond, __cc_left, __cc_right);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Sometimes,
            __cc_cond,
            _ID,
            $msg,
            &__cc_det,
        );
    }};
    ($left:expr, $right:expr, $msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Sometimes,
            $left == $right,
            _ID,
            $msg,
            $details,
        );
    }};
}

/// Assert `left != right` sometimes holds. Auto-captures left/right on failure.
#[macro_export]
macro_rules! cc_assert_sometimes_ne {
    ($left:expr, $right:expr, $msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        let __cc_left = $left;
        let __cc_right = $right;
        let __cc_cond = __cc_left != __cc_right;
        let __cc_det = $crate::__cc_cmp_details!(__cc_cond, __cc_left, __cc_right);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Sometimes,
            __cc_cond,
            _ID,
            $msg,
            &__cc_det,
        );
    }};
    ($left:expr, $right:expr, $msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Sometimes,
            $left != $right,
            _ID,
            $msg,
            $details,
        );
    }};
}

/// Assert that an option is `Some` every time. Captures `{"value": "None"}` on failure.
#[macro_export]
macro_rules! cc_assert_always_some {
    ($expr:expr, $msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        let __cc_val = $expr;
        let __cc_cond = __cc_val.is_some();
        let __cc_det = $crate::__cc_option_details!(__cc_cond);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Always,
            __cc_cond,
            _ID,
            $msg,
            &__cc_det,
        );
    }};
    ($expr:expr, $msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Always,
            $expr.is_some(),
            _ID,
            $msg,
            $details,
        );
    }};
}

/// Assert that an option is `Some` at least once across runs. Captures `{"value": "None"}` on failure.
#[macro_export]
macro_rules! cc_assert_sometimes_some {
    ($expr:expr, $msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        let __cc_val = $expr;
        let __cc_cond = __cc_val.is_some();
        let __cc_det = $crate::__cc_option_details!(__cc_cond);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Sometimes,
            __cc_cond,
            _ID,
            $msg,
            &__cc_det,
        );
    }};
    ($expr:expr, $msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Sometimes,
            $expr.is_some(),
            _ID,
            $msg,
            $details,
        );
    }};
}

// ═══════════════════════════════════════════════════════════════════════
//  Implication macro
// ═══════════════════════════════════════════════════════════════════════

/// Internal macro: build implication details on failure.
#[cfg(feature = "full")]
#[doc(hidden)]
#[macro_export]
macro_rules! __cc_implies_details {
    ($cond:expr, $p:ident, $q:ident) => {
        if $cond {
            $crate::serde_json::json!({})
        } else {
            $crate::serde_json::json!({
                "precondition": format!("{:?}", $p),
                "conclusion": format!("{:?}", $q),
            })
        }
    };
}

#[cfg(not(feature = "full"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __cc_implies_details {
    ($cond:expr, $p:ident, $q:ident) => {
        ()
    };
}

/// Assert that `precondition → conclusion` always holds.
///
/// Equivalent to `cc_assert_always!(!p || q, ...)` but more readable.
/// Auto-captures `precondition` and `conclusion` values on failure.
///
/// ```rust,ignore
/// cc_assert_implies!(node.is_leader(), node.has_log(), "leaders have logs");
/// ```
#[macro_export]
macro_rules! cc_assert_implies {
    ($precondition:expr, $conclusion:expr, $msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        let __cc_p = $precondition;
        let __cc_q = $conclusion;
        let __cc_cond = !__cc_p || __cc_q;
        let __cc_det = $crate::__cc_implies_details!(__cc_cond, __cc_p, __cc_q);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Always,
            __cc_cond,
            _ID,
            $msg,
            &__cc_det,
        );
    }};
    ($precondition:expr, $conclusion:expr, $msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Always,
            !$precondition || $conclusion,
            _ID,
            $msg,
            $details,
        );
    }};
}

// ═══════════════════════════════════════════════════════════════════════
//  Result assertion macros
// ═══════════════════════════════════════════════════════════════════════

/// Internal macro: build Result-error details on failure.
#[cfg(feature = "full")]
#[doc(hidden)]
#[macro_export]
macro_rules! __cc_result_err_details {
    ($cond:expr, $result:ident) => {
        if $cond {
            $crate::serde_json::json!({})
        } else {
            $crate::serde_json::json!({
                "error": format!("{:?}", $result),
            })
        }
    };
}

#[cfg(not(feature = "full"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __cc_result_err_details {
    ($cond:expr, $result:ident) => {
        ()
    };
}

/// Internal macro: build Result-ok details on failure (for _err macros).
#[cfg(feature = "full")]
#[doc(hidden)]
#[macro_export]
macro_rules! __cc_result_ok_details {
    ($cond:expr, $result:ident) => {
        if $cond {
            $crate::serde_json::json!({})
        } else {
            $crate::serde_json::json!({
                "ok_value": format!("{:?}", $result),
            })
        }
    };
}

#[cfg(not(feature = "full"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __cc_result_ok_details {
    ($cond:expr, $result:ident) => {
        ()
    };
}

/// Assert a `Result` is `Ok` every time. Captures error on failure.
///
/// ```rust,ignore
/// cc_assert_always_ok!(channel.try_recv(), "receive succeeds");
/// ```
#[macro_export]
macro_rules! cc_assert_always_ok {
    ($result:expr, $msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        let __cc_result = $result;
        let __cc_cond = __cc_result.is_ok();
        let __cc_det = $crate::__cc_result_err_details!(__cc_cond, __cc_result);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Always,
            __cc_cond,
            _ID,
            $msg,
            &__cc_det,
        );
    }};
    ($result:expr, $msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Always,
            $result.is_ok(),
            _ID,
            $msg,
            $details,
        );
    }};
}

/// Assert a `Result` is `Ok` at least once across runs. Captures error on failure.
#[macro_export]
macro_rules! cc_assert_sometimes_ok {
    ($result:expr, $msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        let __cc_result = $result;
        let __cc_cond = __cc_result.is_ok();
        let __cc_det = $crate::__cc_result_err_details!(__cc_cond, __cc_result);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Sometimes,
            __cc_cond,
            _ID,
            $msg,
            &__cc_det,
        );
    }};
    ($result:expr, $msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Sometimes,
            $result.is_ok(),
            _ID,
            $msg,
            $details,
        );
    }};
}

/// Assert a `Result` is `Err` every time. Captures ok value on failure.
///
/// ```rust,ignore
/// cc_assert_always_err!(parse("not a number"), "rejects bad input");
/// ```
#[macro_export]
macro_rules! cc_assert_always_err {
    ($result:expr, $msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        let __cc_result = $result;
        let __cc_cond = __cc_result.is_err();
        let __cc_det = $crate::__cc_result_ok_details!(__cc_cond, __cc_result);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Always,
            __cc_cond,
            _ID,
            $msg,
            &__cc_det,
        );
    }};
    ($result:expr, $msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_ALWAYS);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Always,
            $result.is_err(),
            _ID,
            $msg,
            $details,
        );
    }};
}

/// Assert a `Result` is `Err` at least once across runs. Captures ok value on failure.
#[macro_export]
macro_rules! cc_assert_sometimes_err {
    ($result:expr, $msg:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        let __cc_result = $result;
        let __cc_cond = __cc_result.is_err();
        let __cc_det = $crate::__cc_result_ok_details!(__cc_cond, __cc_result);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Sometimes,
            __cc_cond,
            _ID,
            $msg,
            &__cc_det,
        );
    }};
    ($result:expr, $msg:expr, $details:expr $(,)?) => {{
        const _ID: u32 =
            $crate::assert::location_id(concat!(file!(), ":", line!(), ":", column!()));
        $crate::__cc_register_catalog!(_ID, $msg, $crate::assert::CATALOG_KIND_SOMETIMES);
        $crate::assert::emit_registered_assertion(
            $crate::assert::AssertionKind::Sometimes,
            $result.is_err(),
            _ID,
            $msg,
            $details,
        );
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
    fn assertion_kind_to_catalog_kind() {
        assert_eq!(AssertionKind::Always.to_catalog_kind(), CATALOG_KIND_ALWAYS);
        assert_eq!(
            AssertionKind::Sometimes.to_catalog_kind(),
            CATALOG_KIND_SOMETIMES,
        );
        assert_eq!(
            AssertionKind::Reachable.to_catalog_kind(),
            CATALOG_KIND_REACHABLE,
        );
        assert_eq!(
            AssertionKind::Unreachable.to_catalog_kind(),
            CATALOG_KIND_UNREACHABLE,
        );
    }

    #[test]
    fn assertion_kind_to_command() {
        use chaoscontrol_protocol::*;
        assert_eq!(AssertionKind::Always.to_command(), CMD_ASSERT_ALWAYS);
        assert_eq!(AssertionKind::Sometimes.to_command(), CMD_ASSERT_SOMETIMES);
        assert_eq!(AssertionKind::Reachable.to_command(), CMD_ASSERT_REACHABLE);
        assert_eq!(
            AssertionKind::Unreachable.to_command(),
            CMD_ASSERT_UNREACHABLE,
        );
    }

    #[test]
    fn assert_raw_macro_compiles() {
        cc_assert_raw!(AssertionKind::Always, true, "raw always");
        cc_assert_raw!(AssertionKind::Sometimes, true, "raw sometimes");
        cc_assert_raw!(AssertionKind::Reachable, true, "raw reachable");
        cc_assert_raw!(AssertionKind::Unreachable, true, "raw unreachable");
    }

    #[test]
    fn assert_raw_macro_with_details() {
        use serde_json::json;
        cc_assert_raw!(
            AssertionKind::Always,
            true,
            "raw details",
            &json!({"k": "v"})
        );
        cc_assert_raw!(AssertionKind::Sometimes, false, "raw details 2", &json!({}),);
    }

    #[test]
    fn comparison_macros_with_explicit_details() {
        use serde_json::json;
        cc_assert_always_lt!(1, 2, "lt", &json!({"custom": true}));
        cc_assert_always_le!(1, 2, "le", &json!({"custom": true}));
        cc_assert_always_gt!(2, 1, "gt", &json!({"custom": true}));
        cc_assert_always_ge!(2, 1, "ge", &json!({"custom": true}));
        cc_assert_always_eq!(1, 1, "eq", &json!({"custom": true}));
        cc_assert_always_ne!(1, 2, "ne", &json!({"custom": true}));
        cc_assert_sometimes_lt!(1, 2, "slt", &json!({"custom": true}));
        cc_assert_sometimes_le!(1, 2, "sle", &json!({"custom": true}));
        cc_assert_sometimes_gt!(2, 1, "sgt", &json!({"custom": true}));
        cc_assert_sometimes_ge!(2, 1, "sge", &json!({"custom": true}));
        cc_assert_sometimes_eq!(1, 1, "seq", &json!({"custom": true}));
        cc_assert_sometimes_ne!(1, 2, "sne", &json!({"custom": true}));
    }

    #[test]
    fn comparison_macros_trailing_comma() {
        cc_assert_always_lt!(1, 2, "trailing",);
        cc_assert_always_le!(1, 2, "trailing",);
    }

    #[test]
    fn comparison_macros_evaluate_operands_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);

        fn inc() -> usize {
            COUNTER.fetch_add(1, Ordering::SeqCst)
        }

        COUNTER.store(0, Ordering::SeqCst);
        // inc() returns 0 on first call, so 0 < 10 passes
        cc_assert_always_lt!(inc(), 10, "operand eval once");
        // Should have been called exactly once
        assert_eq!(COUNTER.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn option_macros_with_explicit_details() {
        use serde_json::json;
        cc_assert_always_some!(Some(1), "some", &json!({"x": 1}));
        cc_assert_sometimes_some!(Some(1), "some2", &json!({"x": 2}));
    }

    // ── Implies macro tests ────────────────────────────────────────

    #[test]
    fn implies_both_true() {
        cc_assert_implies!(true, true, "p implies q");
    }

    #[test]
    fn implies_precondition_false_vacuous() {
        // p=false, q=false → !p || q = true (vacuously true)
        cc_assert_implies!(false, false, "vacuous truth");
    }

    #[test]
    fn implies_precondition_false_conclusion_true() {
        cc_assert_implies!(false, true, "trivially true");
    }

    #[test]
    fn implies_with_explicit_details() {
        use serde_json::json;
        cc_assert_implies!(true, true, "with details", &json!({"node": 3}));
    }

    #[test]
    fn implies_evaluates_operands_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static CTR: AtomicUsize = AtomicUsize::new(0);

        fn check() -> bool {
            CTR.fetch_add(1, Ordering::SeqCst);
            true
        }

        CTR.store(0, Ordering::SeqCst);
        cc_assert_implies!(check(), true, "eval once");
        assert_eq!(CTR.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn implies_trailing_comma() {
        cc_assert_implies!(true, true, "trailing",);
    }

    // ── Result macro tests ─────────────────────────────────────────

    #[test]
    fn always_ok_passes_on_ok() {
        cc_assert_always_ok!(Ok::<i32, String>(42), "op ok");
    }

    #[test]
    fn always_ok_fails_on_err() {
        // This fires the assertion with condition=false, which is fine —
        // we're testing that it compiles and runs, not that VM catches it.
        cc_assert_always_ok!(Err::<i32, String>("timeout".into()), "op ok err");
    }

    #[test]
    fn sometimes_ok_passes_on_ok() {
        cc_assert_sometimes_ok!(Ok::<i32, &str>(1), "write ok");
    }

    #[test]
    fn sometimes_ok_records_err() {
        cc_assert_sometimes_ok!(Err::<i32, &str>("fail"), "write ok err");
    }

    #[test]
    fn always_err_passes_on_err() {
        cc_assert_always_err!(Err::<i32, &str>("expected"), "rejects");
    }

    #[test]
    fn always_err_fails_on_ok() {
        cc_assert_always_err!(Ok::<i32, &str>(42), "rejects ok");
    }

    #[test]
    fn sometimes_err_passes_on_err() {
        cc_assert_sometimes_err!(Err::<i32, &str>("fail"), "error path");
    }

    #[test]
    fn sometimes_err_records_ok() {
        cc_assert_sometimes_err!(Ok::<i32, &str>(1), "error path ok");
    }

    #[test]
    fn result_macros_with_explicit_details() {
        use serde_json::json;
        cc_assert_always_ok!(Ok::<i32, &str>(1), "ok det", &json!({"n": 1}));
        cc_assert_sometimes_ok!(Ok::<i32, &str>(1), "sok det", &json!({"n": 2}));
        cc_assert_always_err!(Err::<i32, &str>("e"), "err det", &json!({"n": 3}));
        cc_assert_sometimes_err!(Err::<i32, &str>("e"), "serr det", &json!({"n": 4}));
    }

    #[test]
    fn result_macros_evaluate_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static CTR: AtomicUsize = AtomicUsize::new(0);

        fn try_op() -> Result<i32, &'static str> {
            CTR.fetch_add(1, Ordering::SeqCst);
            Ok(1)
        }

        CTR.store(0, Ordering::SeqCst);
        cc_assert_always_ok!(try_op(), "eval once");
        assert_eq!(CTR.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn result_macros_trailing_comma() {
        cc_assert_always_ok!(Ok::<i32, &str>(1), "trailing",);
        cc_assert_always_err!(Err::<i32, &str>("e"), "trailing",);
    }

    // no_std stubs compile without the `full` feature.
    // Verified by building with `default-features = false` in CI.
}
