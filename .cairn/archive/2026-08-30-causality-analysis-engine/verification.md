# Verification

Date: 2026-08-30

## Implementation

- `chaoscontrol-sim-core::causality` owns typed step and candidate models, bounded delta debugging, and probable-cause ranking.
- The pure core imports no filesystem, process, clock, network, or environment APIs.
- Delta debugging tests the empty delta, then deterministic complements of the current reproducing step set.
- Every candidate has a BLAKE3 identity. Stale or substituted execution outcomes do not mutate minimization state.
- Budget exhaustion returns the current reproducing set as an explicit partial result.
- Attribution ranks seed, fault-schedule, declared-event, and variant-policy candidates from neutralization outcomes.
- Equivalent outcomes produce no probable cause. Rankings state that they are probability estimates, not proof.
- `chaoscontrol-evidence::causality_shell` owns execution orchestration through the `CausalityExecutor` port.
- The shell validates replay-verdict and snapshot identities after every candidate execution.
- Receipt validation replays the minimization state and attribution ranking from the recorded executions.
- Bounded regular-file readers validate stored request and receipt artifacts without following symbolic links.

## Positive and negative evidence

Positive fixtures minimize a known ordering failure to one step and rank its declared-event cause first.

Negative fixtures cover budget exhaustion, equivalent rankings, stale candidate identity, malformed attempt order, executor failure, replay identity drift, snapshot drift, symbolic-link input, and receipt tampering.

## Checks

- The preceding verified main revision passed 61 simulation-core tests and had no causality-focused evidence tests.
- Post-change simulation core passed 66 tests, including 5 causality fixtures.
- The causality shell passed 3 focused integration tests.
- `cargo test --workspace --all-targets --all-features`: passed, including the available KVM tests.
- Focused and workspace Clippy with `-D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- `git diff --check`: passed.
- Cairn repository validation and the proposal, design, and tasks gates passed.
- Cairn sync preflight reported no blocker.
- Cairn sync and archive execution completed, followed by a successful repository validation.

## Bounded Nix blocker

The Nix evidence-contract build reaches the existing VM Cohort vendor defect before its target. The published `vm-cohort-conformance` package omits `config/generated/profile.json`, which its `include_bytes!` call requires.

This blocker is outside this change. No warning budget, baseline, or weakened gate was added.

## Claim boundary

The analysis is bounded to supplied replay outcomes, candidates, identities, and budgets. It does not prove a unique cause, complete minimization, program correctness, or release eligibility.
