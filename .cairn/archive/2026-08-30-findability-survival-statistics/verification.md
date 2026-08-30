# Verification

Date: 2026-08-30

## Implementation

- `chaoscontrol-sim-core::findability` owns typed subtree observations, first-bug assembly, model policy, statistics, and BLAKE3 report validation.
- The pure core imports no filesystem, process, clock, network, or environment APIs.
- Assembly sorts each subtree's bug instances, keeps the first, and records the discarded count.
- One report rejects mixed generations, duplicate subtrees, malformed identities, zero exposure, and values outside exact model representation.
- The exponential fit reports `M / T` and `T / M`.
- A zero-bug generation reports `no_bug_observed` without a finite rate or confidence projection.
- The gamma posterior emits a conservative Lomax survival tail and a bounded additional-run projection.
- Single samples, baked-in bugs, and correlated independence groups emit no confidence projection.
- `chaoscontrol-evidence::findability_shell` reads bounded regular round artifacts without following symbolic links.
- BLAKE3 identities bind the round artifact, assembled observations, observation set, model policy, and report.

## Positive and negative evidence

The known fixture proves the exact fitted rate and mean, a finite Lomax survival probability, and a bounded projection.

Negative fixtures cover empty input, one sample, duplicate bug instances, no-bug generations, baked-in bugs, correlated groups, invalid policy, symbolic links, artifact identity drift, report identity drift, and duplicate publication.

## Checks

- Pre-change baseline passed 55 simulation-core tests. No findability-focused evidence tests existed.
- Post-change simulation core passed 61 tests, including 6 findability fixtures.
- The findability shell passed 2 focused integration tests.
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

The report is a bounded statistical model over supplied first-bug observations. It does not prove bug absence, code correctness, deterministic replay, independent sampling, or release eligibility.
