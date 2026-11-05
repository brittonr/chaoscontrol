## Why

The guest diagnostic request contract admits an executable identity, arguments, and
timeout, but nothing executes an admitted request. The supervisor needs a bounded
runner that verifies the identity before spawning and reports a receipt-grade
outcome.

## What Changes

Add `chaoscontrol_sdk::diagnostic::run_diagnostic`: validate the request, hash the
supervisor-chosen executable and compare it to the requested identity, spawn it with
null stdin and captured output, enforce the request timeout by killing the process,
cap each captured stream, and report the request digest, termination class, exit
code, and output lengths/digests. The caller must still bind the outcome into a
receipt; this change does not prove executable trust or output correctness.

## Impact

- **Files**: `crates/chaoscontrol-sdk/src/diagnostic.rs`,
  `crates/chaoscontrol-sdk/src/lib.rs`, `crates/chaoscontrol-sdk/Cargo.toml`
- **Testing**: SDK unit tests cover a successful bounded run, identity rejection
  before spawn, and timeout kill
