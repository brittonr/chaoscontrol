## Why

The SDK covers the Antithesis assertion and randomness APIs well, but the gap analysis identified several missing features that limit explorer effectiveness and guest ergonomics. The biggest gap is guidance — the protocol defines `CMD_GUIDANCE` (0x07) but nothing sends it, receives it, or acts on it, leaving the explorer blind to "how close" the guest is to violating a property. Secondary gaps are ergonomic macros that the existing guests work around with boilerplate.

## What Changes

- Add a guidance API to the SDK so guests can send numeric distance-to-violation hints to the VMM, and wire the VMM side to receive and store them for explorer consumption.
- Add comparison macro variants that auto-capture left/right values in assertion details on failure.
- Add `cc_assert_implies!` macro for precondition→conclusion properties.
- Add `cc_assert_always_ok!` / `cc_assert_sometimes_ok!` / `cc_assert_always_err!` / `cc_assert_sometimes_err!` macros for Result-aware assertions.

## Capabilities

### New Capabilities
- `sdk-guidance`: Guest-to-VMM guidance channel — SDK functions to emit numeric hints, protocol handling in the fault engine, and storage for explorer consumption.
- `sdk-comparison-details`: Comparison assertion macros (`_lt`, `_le`, `_gt`, `_ge`, `_eq`, `_ne`) auto-capture the actual left/right values in assertion details on failure.
- `sdk-assert-implies`: `cc_assert_implies!` macro that expresses `precondition → conclusion` properties without manual De Morgan encoding.
- `sdk-assert-result`: Result-aware assertion macros (`cc_assert_always_ok!`, `cc_assert_sometimes_ok!`, `cc_assert_always_err!`, `cc_assert_sometimes_err!`) that auto-capture the error/ok value in details.

### Modified Capabilities

## Impact

- `chaoscontrol-sdk`: New public functions and macros in `assert.rs`, new `guidance.rs` module.
- `chaoscontrol-protocol`: `CMD_GUIDANCE` already defined; may need additional payload encoding for guidance data.
- `chaoscontrol-fault`: `engine.rs` `handle_hypercall` needs a `CMD_GUIDANCE` arm; `oracle.rs` or a new structure needs to store guidance values.
- `chaoscontrol-explore`: Future consumer of guidance data (not part of this change, but the storage must be accessible).
- Existing guest crates (`chaoscontrol-guest`, `chaoscontrol-raft-guest`, `chaoscontrol-net-guest`): Can adopt new macros but no breakage — all additions are additive.
