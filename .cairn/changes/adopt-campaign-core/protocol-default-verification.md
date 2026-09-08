# Explicit branch-marker absence

## Change

The focused Octet check rejected Serde's implicit external defaults for `state_ref` and `logical_position_ref`.

The protocol now owns their missing-field value through `absent_marker_reference`, which returns `None`. Explicit references remain unchanged. Missing or null references carry no snapshot or logical-position claim. Malformed field types still fail decoding.

Three regression tests cover missing and null fields, explicit reference round trips, and malformed types for both fields. Existing identity and bounds tests remain intact.

Two test-only constants now live inside their existing standard-library tests. This fixes default-feature Clippy errors without changing test conditions, values, or assertions.

## Checks and limits

- The initial default-feature baseline passed 15 tests. It did not exercise the standard-library branch-marker module.
- The corrected baseline ran against frozen pre-change consumer `c5448745d66fc3945aadce83da9064abe961e13c`. It passed 35 all-feature protocol tests. This baseline ran after the initial edit, not before it.
- The changed protocol passed 38 all-feature tests and 15 no-default-feature tests. No tests were ignored.
- Strict Clippy passed with all targets for both feature configurations.
- The focused Octet check advanced past the protocol errors. It now fails on existing defaults in `chaoscontrol-fault/src/engine.rs` and `chaoscontrol-fault/src/oracle.rs`.

No lint was disabled or suppressed. The focused Octet check and full workspace acceptance remain incomplete.

## Logs

The operator retains these logs under `campaign/.pi/complete-20260906/`:

- `protocol-default-baseline.log`
- `protocol-default-std-baseline-final.log`
- `protocol-default-all-feature-tests.log`
- `protocol-default-no-feature-tests.log`
- `protocol-default-all-feature-clippy.log`
- `protocol-default-no-feature-clippy.log`
- `protocol-default-octet.log`
