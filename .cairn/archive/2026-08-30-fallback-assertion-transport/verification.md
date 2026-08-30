# Verification

Date: 2026-08-30

## Implementation

- `chaoscontrol-protocol` owns the closed schema, canonical record identity, bounded sink, overflow event, order validation, and combined catalog admission.
- BLAKE3 identities bind canonical JSON details, record order, the sink limit, and overflow state.
- `chaoscontrol-fault` activates the combined catalog and ingests each sink as a transactional oracle update.
- Catalog conflicts report the process, sequence, candidate fingerprint, existing fingerprint, and typed conflict.
- Oracle records retain the process scope for the failing fallback record.
- Bug reports, checkpoints, and replay verdicts retain and validate `fallback_scope`.
- Nickel review contracts reject missing fallback scope and scope attached to a normal SDK assertion.
- The Rust SDK path and its catalog authority remain unchanged.

## Positive and negative evidence

Positive fixtures cover ordered assertion and lifecycle ingestion, catalog admission, a failed property, overflow evidence, bug scope, replay scope, and canonical key-order independence.

Negative fixtures cover malformed records, missing process identity, reordered records, closed sinks, sink identity drift, catalog conflict, overflow, missing scope, process drift, and process-scope overclaim.

## Checks

- Pre-change focused tests passed: 201 explorer tests, 96 fault tests, 20 protocol tests, and 28 replay-core tests. One declared explorer integration test remained ignored.
- Post-change focused tests passed: 26 protocol tests, 96 fault unit tests plus the two fallback integration tests, 23 replay-core fixtures, and 202 explorer tests. One declared explorer integration test remained ignored.
- `cargo test --workspace --all-targets --all-features`: passed, including the available KVM tests.
- Focused and workspace Clippy with `-D warnings`: passed.
- `cargo check -p chaoscontrol-protocol --no-default-features`: passed.
- `cargo fmt --all -- --check`: passed.
- `git diff --check`: passed.
- Nickel type checks and the two new negative contract fixtures passed.
- Cairn repository validation and the proposal, design, and tasks gates passed.
- Cairn sync preflight reported no blocker.
- Cairn sync and archive execution completed, followed by a successful repository validation.

## Bounded blockers

The Nix evidence-contract build reaches the existing VM Cohort vendor defect before its target. The published `vm-cohort-conformance` package omits `config/generated/profile.json`, which its `include_bytes!` call requires.

The direct evidence-contract checker stops on the existing active `add-protocol-observation-cohorts` package because that package lacks a product-scope intent. This stop occurs before the checker reaches the complete contract suite.

These blockers are outside this change. No warning budget, baseline, or weakened gate was added.

## Claim boundary

The fallback path proves bounded, ordered, process-scoped assertion and lifecycle ingestion for supplied records. It does not provide code coverage, a language SDK, whole-guest truth, arbitrary process supervision, or universal replay correctness.
