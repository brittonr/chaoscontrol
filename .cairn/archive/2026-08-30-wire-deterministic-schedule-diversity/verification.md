# Verification

Date: 2026-08-30

## Behavior

- Normal and havoc exploration paths generate fault-schedule and scheduler-variant pairs when SMP diversity is enabled.
- CLI SMP runs set an explicit full schedule-mutation ratio and bind the selected base quantum.
- Parallel `BranchWork` and sequential branches preserve and apply each variant before the counterfactual fault run.
- Invalid parallel variants return the typed scheduler error instead of a zero-coverage placeholder.
- Scheduler fingerprints change for seed drift and bind the applied strategy and quantum state.
- Bug reports retain the complete variant.
- Replay verdicts retain seed, strategy, quantum override, and BLAKE3 policy identity.
- Human reports state that no-bug output does not prove validated or exhaustive interleaving coverage.

## Tests

- Pre-change `chaoscontrol-sim-core` and `chaoscontrol-explore` tests passed.
- Focused post-change tests passed: 201 explorer tests, 55 simulation-core tests, and 28 replay-core tests. One declared explorer integration test remains ignored.
- The known-race fixture proves the default quantum misses the race window and a generated variant reaches it.
- Negative fixtures passed for disabled diversity, single-vCPU admission, invalid strategy bounds, zero quantum, malformed replay evidence, and policy identity drift.
- `cargo test -p chaoscontrol-vmm`: passed 486 tests with 9 declared KVM-dependent ignores.
- `cargo test --workspace --all-targets --all-features`: passed.
- Focused and workspace Clippy with `-D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- Cairn validation and the proposal, design, and tasks gates passed.
- Cairn sync and archive execution completed, followed by a successful repository validation.

## Nix observations

The Nix formatting check passed. The broader evidence-contract check reached the existing VM Cohort vendor defect before running its target. The published `vm-cohort-conformance` package omits `config/generated/profile.json` used by `include_bytes!`.

The focused Tiger Style check reports the repository's existing finding set. No baseline or warning budget was added. Local strict Clippy and the changed focused tests are clean.

## Claim boundary

The race fixture proves one bounded scheduler mechanism. It does not prove exhaustive interleaving coverage, application correctness, host determinism, or absence of races.
