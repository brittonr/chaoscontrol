## Why

The SDK assertion macros (`cc_assert_always!`, etc.) cover the common case, but
users with existing test frameworks — proptest, quickcheck, custom harnesses — have
no way to route their results into ChaosControl's property oracle. They must rewrite
all assertions using our macros or lose cross-run verdict tracking, assertion catalog
registration, and exploration report coverage entirely. `assert_raw()` is the escape
hatch: a single function that accepts pre-computed assertion parameters and feeds them
straight into the oracle, letting any framework participate in the catalog.

## What Changes

- Add `assert_raw()` and `assert_raw_with_id()` to `chaoscontrol-sdk` as public API
  functions that accept assertion kind, condition, ID, message, and details.
- Add a `cc_assert_raw!` macro variant that registers in the assertion catalog via
  `linkme` (same as the typed macros) but takes kind as a parameter.
- Wire through the existing `CMD_ASSERT_ALWAYS` / `CMD_ASSERT_SOMETIMES` /
  `CMD_ASSERT_REACHABLE` / `CMD_ASSERT_UNREACHABLE` transport commands based on
  the `kind` argument — no protocol changes needed.
- Add `AssertionKind` enum to the SDK's public API for callers to specify the
  assertion type (re-export or mirror the fault crate's enum, kept `no_std`
  compatible).

## Capabilities

### New Capabilities
- `assert-raw`: Low-level assertion function that accepts kind + condition + id + message + details and dispatches to the oracle. Enables third-party framework integration without requiring macro usage.

### Modified Capabilities

(none — existing assertion macros and oracle are unchanged)

## Impact

- `chaoscontrol-sdk`: new public functions + macro + `AssertionKind` enum
- `chaoscontrol-protocol`: no changes (reuses existing command IDs)
- `chaoscontrol-fault`: no changes (oracle already handles all 4 assertion kinds)
- Guest programs: no changes required (opt-in API)
- Breaking: none
