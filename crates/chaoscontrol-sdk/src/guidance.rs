//! Antithesis-compatible guidance records.
//!
//! Guidance reports the values behind a property so an explorer can prefer
//! extreme violations. ChaosControl records the same `antithesis_guidance`
//! JSON shape as a structured event; it does not yet emit the guidance
//! catalog at initialization.

use core::sync::atomic::{AtomicU64, Ordering};
use serde_json::Value;

/// One guidance site collected at compile time.
pub struct GuidanceCatalogEntry {
    pub id: u32,
    pub kind: crate::assert::AssertionKind,
    pub message: &'static str,
    pub guidance_type: &'static str,
    pub maximize: bool,
    pub class: &'static str,
    pub function: fn() -> &'static str,
    pub file: &'static str,
    pub line: u32,
    pub column: u32,
}

/// Every guidance site in the linked guest.
#[linkme::distributed_slice]
pub static GUIDANCE_CATALOG: [GuidanceCatalogEntry];

#[doc(hidden)]
#[macro_export]
macro_rules! __cc_register_guidance {
    ($id:expr, $kind:expr, $msg:expr, $guidance_type:expr, $maximize:expr, $class:expr, $file:expr, $line:expr, $column:expr) => {
        fn __cc_guidance_function_name() -> &'static str {
            fn __cc_guidance_probe() {}
            let name = ::core::any::type_name_of_val(&__cc_guidance_probe);
            &name[..name.len() - "::__cc_guidance_function_name::__cc_guidance_probe".len()]
        }
        const _: () = {
            #[linkme::distributed_slice($crate::guidance::GUIDANCE_CATALOG)]
            static _GUIDANCE_ENTRY: $crate::guidance::GuidanceCatalogEntry =
                $crate::guidance::GuidanceCatalogEntry {
                    id: $id,
                    kind: $kind,
                    message: $msg,
                    guidance_type: $guidance_type,
                    maximize: $maximize,
                    class: $class,
                    function: __cc_guidance_function_name as fn() -> &'static str,
                    file: $file,
                    line: $line,
                    column: $column,
                };
        };
    };
}

/// Difference between two operands, used for numeric guidance watermarks.
pub trait GuidanceDiff {
    /// Signed distance from `other` to `self`.
    fn diff(&self, other: &Self) -> f64;
}

macro_rules! impl_diff_unsigned {
    ($($t:ty)*) => {$(
        impl GuidanceDiff for $t {
            fn diff(&self, other: &Self) -> f64 {
                if self < other {
                    -((*other - *self) as f64)
                } else {
                    (*self - *other) as f64
                }
            }
        }
    )*};
}

impl_diff_unsigned! { usize u8 u16 u32 u64 u128 }

macro_rules! impl_diff_signed {
    ($(($t:ty, $unsigned:ty))*) => {$(
        impl GuidanceDiff for $t {
            fn diff(&self, other: &Self) -> f64 {
                if self < other {
                    -((*other as $unsigned).wrapping_sub(*self as $unsigned) as f64)
                } else {
                    ((*self as $unsigned).wrapping_sub(*other as $unsigned)) as f64
                }
            }
        }
    )*};
}

impl_diff_signed! { (isize, usize) (i8, u8) (i16, u16) (i32, u32) (i64, u64) (i128, u128) }

macro_rules! impl_diff_float {
    ($($t:ty)*) => {$(
        impl GuidanceDiff for $t {
            fn diff(&self, other: &Self) -> f64 {
                ((*self - *other) as f64)
            }
        }
    )*};
}

impl_diff_float! { f32 f64 }

/// Per-site watermark that emits only new extremal values.
pub struct NumericGuard<const MAXIMIZE: bool> {
    bits: AtomicU64,
}

impl<const MAXIMIZE: bool> NumericGuard<MAXIMIZE> {
    /// Create a guard initialized to the opposite extreme.
    pub const fn new() -> Self {
        let initial = if MAXIMIZE {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
        Self {
            bits: AtomicU64::new(initial.to_bits()),
        }
    }

    /// Return true when `value` is a new extreme for this site.
    pub fn should_emit(&self, value: f64) -> bool {
        if value.is_nan() {
            return true;
        }
        loop {
            let current_bits = self.bits.load(Ordering::Relaxed);
            let current = f64::from_bits(current_bits);
            let more_extreme = if MAXIMIZE {
                value > current
            } else {
                value < current
            };
            if !more_extreme {
                return false;
            }
            if self
                .bits
                .compare_exchange_weak(
                    current_bits,
                    value.to_bits(),
                    Ordering::SeqCst,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                return true;
            }
        }
    }
}

impl<const MAXIMIZE: bool> Default for NumericGuard<MAXIMIZE> {
    fn default() -> Self {
        Self::new()
    }
}

/// Emit one numeric guidance record.
#[allow(clippy::too_many_arguments)]
pub fn emit_numeric(
    id: u32,
    kind: crate::assert::AssertionKind,
    message: &str,
    class: &str,
    function: &str,
    file: &str,
    begin_line: u32,
    begin_column: u32,
    maximize: bool,
    guidance_data: Value,
) {
    emit(
        id,
        kind,
        "numeric",
        message,
        class,
        function,
        file,
        begin_line,
        begin_column,
        maximize,
        guidance_data,
        true,
    );
}

/// Emit one boolean guidance record.
#[allow(clippy::too_many_arguments)]
pub fn emit_boolean(
    id: u32,
    kind: crate::assert::AssertionKind,
    message: &str,
    class: &str,
    function: &str,
    file: &str,
    begin_line: u32,
    begin_column: u32,
    maximize: bool,
    guidance_data: Value,
) {
    emit(
        id,
        kind,
        "boolean",
        message,
        class,
        function,
        file,
        begin_line,
        begin_column,
        maximize,
        guidance_data,
        true,
    );
}

/// Emit one JSON guidance record bound to a catalog assertion site.
#[allow(clippy::too_many_arguments)]
pub fn emit_json(
    id: u32,
    kind: crate::assert::AssertionKind,
    message: &str,
    class: &str,
    function: &str,
    file: &str,
    begin_line: u32,
    begin_column: u32,
    maximize: bool,
    guidance_data: Value,
) {
    emit(
        id,
        kind,
        "json",
        message,
        class,
        function,
        file,
        begin_line,
        begin_column,
        maximize,
        guidance_data,
        true,
    );
}

/// Emit JSON guidance for an existing stable assertion site.
///
/// The `namespace`/`key` pair must match a compiled stable assertion with
/// the same kind and message.
#[doc(hidden)]
#[macro_export]
macro_rules! cc_guidance_json {
    ($namespace:expr, $key:expr, $kind:expr, $message:expr, $maximize:expr, $data:expr) => {{
        fn __cc_json_guidance_function_name() -> &'static str {
            fn __cc_json_guidance_probe() {}
            let name = ::core::any::type_name_of_val(&__cc_json_guidance_probe);
            &name[..name.len()
                - "::__cc_json_guidance_function_name::__cc_json_guidance_probe".len()]
        }
        let __cc_id = $crate::assert::location_id(&::std::format!("{}:{}", $namespace, $key));
        $crate::guidance::emit_json(
            __cc_id,
            $kind,
            $message,
            module_path!(),
            __cc_json_guidance_function_name(),
            file!(),
            line!(),
            column!(),
            $maximize,
            $data,
        );
    }};
}

/// Emit the initialization-time record for one guidance site.
pub fn emit_catalog_record(entry: &GuidanceCatalogEntry) {
    emit(
        entry.id,
        entry.kind,
        entry.guidance_type,
        entry.message,
        entry.class,
        (entry.function)(),
        entry.file,
        entry.line,
        entry.column,
        entry.maximize,
        Value::Null,
        false,
    );
}

#[allow(clippy::too_many_arguments)]
fn emit(
    id: u32,
    kind: crate::assert::AssertionKind,
    guidance_type: &str,
    message: &str,
    class: &str,
    function: &str,
    file: &str,
    begin_line: u32,
    begin_column: u32,
    maximize: bool,
    guidance_data: Value,
    hit: bool,
) {
    let Some(identity) = crate::assertion_catalog::resolve_compatibility(id, kind, message) else {
        return;
    };
    let record = serde_json::json!({
        "guidance_type": guidance_type,
        "message": message,
        "id": message,
        "location": {
            "class": class,
            "function": function,
            "file": file,
            "begin_line": begin_line,
            "begin_column": begin_column,
        },
        "maximize": maximize,
        "guidance_data": guidance_data,
        "hit": hit,
        "identity_version": chaoscontrol_protocol::identity::ASSERTION_IDENTITY_VERSION,
        "catalog_token": identity.catalog_token,
        "assertion_fingerprint": identity.fingerprint,
        "catalog_status": "accepted",
    });
    crate::lifecycle::send_event("antithesis_guidance", &record);
}

/// Emit numeric guidance when the per-site watermark accepts `value`.
#[doc(hidden)]
#[macro_export]
macro_rules! __cc_numeric_guidance {
    ($left:ident, $right:ident, $message:expr, $maximize:literal, $file:expr, $line:expr, $column:expr, $class:expr, $kind:expr, $id:expr) => {{
        static __CC_GUARD: $crate::guidance::NumericGuard<$maximize> =
            $crate::guidance::NumericGuard::new();
        let __cc_diff = $crate::guidance::GuidanceDiff::diff(&$left, &$right);
        if __CC_GUARD.should_emit(__cc_diff) {
            $crate::guidance::emit_numeric(
                $id,
                $kind,
                $message,
                $class,
                __cc_guidance_function_name(),
                $file,
                $line,
                $column,
                $maximize,
                $crate::serde_json::json!({
                    "left": $left,
                    "right": $right,
                }),
            );
        }
    }};
}

/// Emit boolean guidance for named conditions.
#[doc(hidden)]
#[macro_export]
macro_rules! __cc_boolean_guidance {
    ($message:expr, $maximize:literal, {$($name:ident: $cond:expr),* $(,)?}, $file:expr, $line:expr, $column:expr, $class:expr, $kind:expr, $id:expr) => {{
        let __cc_guidance = $crate::serde_json::json!({
            $(::core::stringify!($name): $cond,)*
        });
        $crate::guidance::emit_boolean(
            $id,
            $kind,
            $message,
            $class,
            __cc_guidance_function_name(),
            $file,
            $line,
            $column,
            $maximize,
            __cc_guidance,
        );
    }};
}
