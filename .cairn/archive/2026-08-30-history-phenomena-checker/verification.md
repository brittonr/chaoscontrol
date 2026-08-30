# Verification

Date: 2026-08-30

## Implementation

- `chaoscontrol-smr::phenomena` owns the pure typed history, dependency model, BLAKE3 identities, bounded classifications, and report validation.
- The core source imports no filesystem, process, clock, network, or environment APIs.
- The checker classifies aborted, intermediate, garbage, stale, lost-write, and write-cycle observations.
- The write-cycle pass uses an explicit bounded linear graph traversal.
- Every violation carries the responsible operation identities and BLAKE3 bindings.
- Any declared observation gap returns `insufficient_data` with the affected pairs and no invented violation.
- `chaoscontrol-evidence::phenomena_shell` reads bounded regular files without following symbolic links.
- The shell validates native phenomena histories and adapts the existing typed single-register operation-history artifact without parsing raw logs.
- `check-history-phenomena` exposes validation, native checking, existing-round checking, and fail-closed report publication.

## Positive and negative evidence

Positive fixtures cover each named phenomenon, a clean history, native shell ingestion, existing round-artifact adaptation, and report publication.

Negative fixtures cover missing operation identity, non-canonical order, unknown or mismatched observations, observation gaps, symbolic links, history identity drift, report drift, and duplicate report publication.

## Checks

- Pre-change baseline passed 9 SMR tests and 6 focused consistency-checker tests.
- Post-change `chaoscontrol-smr` passed 18 tests, including 9 phenomenon fixtures.
- The phenomenon shell passed 3 focused integration tests.
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

The checker classifies the supplied bounded observations. It is not a complete concurrent-history solver, does not identify the code defect, does not prove deterministic replay, and does not establish release eligibility.
