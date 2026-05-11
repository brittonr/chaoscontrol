## Phase 1: Scope foundation

- [x] [serial] Write scope proposal, design, and delta specs for local/Rust product focus.

## Phase 2: Generated wording and gates

- [ ] [serial] Update replay-readiness generated surfaces so current missing features are local multi-hypervisor and Rust SDK gaps, not SaaS, multi-machine fleet, or multi-language gaps.
- [ ] [parallel] Add negative promotion-gate fixtures for hosted/multi-machine overclaims that remain invalid even when those areas are current non-goals.
- [ ] [parallel] Update Rust workload docs/status snippets to present Rust-only SDK support as intentional current scope.

## Phase 3: Verification

- [ ] [depends:generated-wording] Run focused evidence model tests and readiness report `--check`.
- [ ] [depends:generated-wording] Run `openspec validate --all --strict` and `git diff --check`.
