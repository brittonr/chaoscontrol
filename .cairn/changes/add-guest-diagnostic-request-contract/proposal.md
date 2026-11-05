## Why

GitHub issue #1 asks for opt-in guest diagnostic operations behind the guest
supervisor. The host and supervisor need one versioned, bounded request encoding
before any execution or result plumbing can be reviewed, so that an admitted
executable identity, argument vector, and timeout are fixed at the contract
boundary.

## What Changes

Add `CMD_DIAGNOSTIC_REQUEST` and a `chaoscontrol-protocol::diagnostic` module that
admits and canonically encodes or decodes one diagnostic request: protocol version,
32-byte BLAKE3 executable identity, 1-16 non-empty UTF-8 arguments, and a bounded
timeout. Admission is lexical and bounded. The module does not execute requests,
authorize executables, or produce evidence; supervisor execution and receipt
plumbing remain unimplemented in this slice.

## Impact

- **Files**: `crates/chaoscontrol-protocol/src/lib.rs`,
  `crates/chaoscontrol-protocol/src/diagnostic.rs`
- **Testing**: protocol crate unit tests cover canonical roundtrip and every
  rejection class
