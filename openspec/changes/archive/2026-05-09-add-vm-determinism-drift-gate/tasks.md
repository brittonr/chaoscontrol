## Phase 1: Contract and pure core

- [x] [serial] Create the OpenSpec package for the bounded VM determinism drift gate.
- [x] [parallel] Add a pure Rust comparison/receipt core for VM/controller fingerprints and mismatch classification.
- [x] [parallel] Add unit tests for passing, failing, and aggregate receipt status.

## Phase 2: Stress gate integration

- [x] [serial] Extend `determinism_stress` with receipt emission while preserving existing positional arguments.
- [x] [parallel] Add optional dlog capture/structural comparison for evidence runs.
- [x] [parallel] Record deterministic input fingerprints in the receipt.

## Phase 3: Verification

- [x] [serial] Run focused Rust and OpenSpec validation for the implemented drift gate.
