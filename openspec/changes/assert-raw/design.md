## Context

The SDK currently exposes four typed assertion functions (`always`, `sometimes`,
`reachable`, `unreachable`) and their corresponding `cc_assert_*!` macros. Each
function hardcodes a specific `CMD_ASSERT_*` protocol command. Users who want to
integrate existing test frameworks (proptest generators, quickcheck properties,
custom assertion libraries) must call one of these four functions — there is no
generic entry point that accepts the assertion kind as a parameter.

The transport layer (`hypercall`) and protocol (`CMD_ASSERT_*` commands) already
support all four kinds. The oracle (`PropertyOracle`) already handles all four via
`record_always`, `record_sometimes`, `record_reachable`, `record_unreachable`. No
wire protocol or engine changes are needed.

## Goals / Non-Goals

**Goals:**
- Single `assert_raw()` function that accepts assertion kind + condition + message + details
- `cc_assert_raw!` macro that registers in the linkme assertion catalog
- `AssertionKind` enum in the SDK's public API (no_std compatible)
- No protocol or oracle changes

**Non-Goals:**
- Batch assertion submission (one at a time is fine)
- Custom assertion kinds beyond the four existing ones
- Changing the existing typed macros/functions (they stay as-is)

## Decisions

### 1. `AssertionKind` lives in `chaoscontrol-sdk`, not `chaoscontrol-protocol`

The protocol crate is `no_std` with zero deps. Adding an enum there would work
but forces downstream crates to import protocol directly. Putting it in the SDK
keeps the public API surface in one place. The enum maps to `CMD_ASSERT_*` constants
internally via a `match`.

Alternative: mirror the `chaoscontrol_fault::oracle::AssertionKind` enum. Rejected
because SDK shouldn't depend on the fault crate (guest-side vs host-side).

### 2. `assert_raw()` dispatches through existing `hypercall()` with a `match`

```rust
pub fn assert_raw(kind: AssertionKind, cond: bool, message: &str, details: &serde_json::Value) {
    let id = location_id(message);
    assert_raw_with_id(kind, cond, id, message, details);
}

pub fn assert_raw_with_id(kind: AssertionKind, cond: bool, id: u32, message: &str, details: &serde_json::Value) {
    let (command, flags) = match kind {
        AssertionKind::Always => (CMD_ASSERT_ALWAYS, if cond { 0x01 } else { 0x00 }),
        AssertionKind::Sometimes => (CMD_ASSERT_SOMETIMES, if cond { 0x01 } else { 0x00 }),
        AssertionKind::Reachable => (CMD_ASSERT_REACHABLE, 0x00),
        AssertionKind::Unreachable => (CMD_ASSERT_UNREACHABLE, 0x00),
    };
    let json_bytes = to_json_bytes(details);
    transport::hypercall(command, flags, id, message, &json_bytes);
}
```

Alternative: add a new `CMD_ASSERT_RAW` protocol command that carries the kind
in the payload. Rejected — unnecessary protocol expansion when we already have
four command IDs that map 1:1.

### 3. `cc_assert_raw!` macro delegates to `assert_raw_with_id` + registers in catalog

The macro maps `AssertionKind` to `CATALOG_KIND_*` for catalog registration:

```rust
macro_rules! cc_assert_raw {
    ($kind:expr, $cond:expr, $msg:expr $(,)?) => { ... };
    ($kind:expr, $cond:expr, $msg:expr, $details:expr $(,)?) => { ... };
}
```

The `kind` must be a const-evaluable expression since `CATALOG_KIND_*` mapping
happens at compile time. Use a `const fn kind_to_catalog(kind: AssertionKind) -> u8`.

### 4. Reachable/Unreachable ignore the `cond` parameter

For `AssertionKind::Reachable` and `AssertionKind::Unreachable`, the `cond`
parameter is ignored (reachable fires on any call, unreachable always fails on
call). Document this: `cond` is only meaningful for `Always` and `Sometimes`.

## Risks / Trade-offs

- **[Macro const evaluation]** The catalog registration needs a const `u8` for
  the kind. `AssertionKind` → `CATALOG_KIND_*` mapping must be a `const fn`.
  If the user passes a runtime-computed kind to `cc_assert_raw!`, it won't
  compile. Document that the macro requires a const kind expression. →
  Mitigation: `assert_raw()` function works with runtime kinds (no catalog
  registration); macro requires const kinds.

- **[API surface growth]** Adding more public functions. → Minimal: 2 functions
  + 1 macro + 1 enum. Consistent with existing pattern.
