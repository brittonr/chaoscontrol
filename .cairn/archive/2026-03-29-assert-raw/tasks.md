## 1. AssertionKind Enum

- [x] 1.1 Add `AssertionKind` enum to `crates/chaoscontrol-sdk/src/assert.rs` with `Always`, `Sometimes`, `Reachable`, `Unreachable` variants. Derive `Copy, Clone, Debug, PartialEq, Eq`. Guard with `#[cfg(feature = "full")]` and provide a no_std stub.
- [x] 1.2 Add `const fn to_catalog_kind(&self) -> u8` method mapping each variant to the corresponding `CATALOG_KIND_*` constant.
- [x] 1.3 Add `fn to_command(&self) -> u8` method mapping each variant to the corresponding `CMD_ASSERT_*` protocol constant.
- [x] 1.4 Export `AssertionKind` from the SDK prelude.

## 2. Core Functions

- [x] 2.1 Add `assert_raw(kind, cond, message, details)` in `assert.rs` that computes ID via `location_id(message)` and delegates to `assert_raw_with_id`. Add no_std no-op stub.
- [x] 2.2 Add `assert_raw_with_id(kind, cond, id, message, details)` that uses `kind.to_command()` to select the protocol command, computes flags from `cond` (0x01 if true for Always/Sometimes, 0x00 for Reachable/Unreachable), and calls `transport::hypercall`.
- [x] 2.3 Add `to_json_bytes` call for details serialization (reuse existing helper).

## 3. Macro

- [x] 3.1 Add `cc_assert_raw!` macro with 3-arg `(kind, cond, msg)` and 4-arg `(kind, cond, msg, details)` patterns, both with trailing-comma tolerance `$(,)?`.
- [x] 3.2 Macro registers in `ASSERTION_CATALOG` via `__cc_register_catalog!` using `kind.to_catalog_kind()` for the catalog kind byte.
- [x] 3.3 Add no_std no-op stub for `cc_assert_raw!` when `full` feature is disabled.

## 4. Tests

- [x] 4.1 Unit test: `AssertionKind::to_catalog_kind()` returns correct constants for all 4 variants.
- [x] 4.2 Unit test: `AssertionKind::to_command()` returns correct `CMD_ASSERT_*` for all 4 variants.
- [x] 4.3 Compile test: `cc_assert_raw!` with all 4 kinds compiles (existing macro compile tests pattern).
- [x] 4.4 Compile test: `assert_raw()` and `assert_raw_with_id()` accept `&serde_json::Value` details.
- [x] 4.5 Compile test: no_std stubs compile without `full` feature.

## 5. Documentation

- [x] 5.1 Add doc comments to `assert_raw`, `assert_raw_with_id`, `cc_assert_raw!`, and `AssertionKind` with usage examples showing third-party framework integration.
- [x] 5.2 Update `crates/chaoscontrol-sdk/src/prelude.rs` to re-export `AssertionKind`.
