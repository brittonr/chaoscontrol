## Phase 1: Spec foundation

- [x] [serial] Create the OpenSpec package for the device/profile determinism matrix gap.

## Phase 2: Matrix model and validation

- [x] [serial] Add Rust-owned matrix config and receipt models for guest/device/profile rows.
- [x] [parallel] Add pure aggregation tests for passing, failing, missing, and duplicate matrix rows.
- [x] [parallel] Add negative drift fixtures proving the validator rejects mismatched fingerprints and weakened anti-claims.

## Phase 3: Runner and packaging

- [x] [depends:matrix-model] Wire a bounded matrix CLI/runner around existing VM determinism drift observations.
- [x] [depends:runner] Package a small Nix matrix rail that emits matrix receipt and summary artifacts.

## Phase 4: Status and promotion gate

- [x] [depends:packaging] Regenerate readiness/status docs with a bounded matrix label, not a universal determinism claim.
- [x] [depends:status] Add promotion-gate checks that fail if arbitrary determinism is claimed without matrix evidence.
- [x] [depends:promotion-gate] Verify with focused cargo tests, report `--check`, OpenSpec validation, and the packaged drift rail.
