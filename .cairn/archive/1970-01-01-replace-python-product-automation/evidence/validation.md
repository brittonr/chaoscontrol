# Validation evidence

## Inventory and parity

- All five former Python scripts and four inline blocks have recorded owners, inputs, outputs, exit classes, bounds, effects, callers, and tests in `docs/rust-product-automation.md`.
- Old and new accepted dogfood refresh outputs match after canonical JSON comparison: PASS.
- Old and new dogfood text and JSON summaries match byte for byte: PASS.
- Old and new cargo-audit self-test output matches byte for byte: PASS.
- Old and new materialized `run-config.json` matches byte for byte: PASS.
- Old and new materialized `receipt.json` matches after canonical JSON comparison: PASS.
- Rust core tests cover the removed inline matrix, replay receipt, scaffold, and drift decisions: PASS.

## Positive and negative checks

The Rust tests and focused Nix check cover accepted inputs, malformed JSON, missing fields, stale snapshot facts, untriaged and stale audit entries, duplicate workloads, mismatched expectations, unsafe paths, existing destinations, read-only destinations, output truncation policy, timeouts, and caller drift.

The source guard reports no Python product scripts or Python references in `flake.nix`. It also requires every focused Rust binary.

## Focused validation

- `cargo test -p chaoscontrol-evidence`: PASS, 139 tests.
- `cargo clippy -p chaoscontrol-evidence --all-targets -- -D warnings`: PASS.
- Rust and Nix formatting: PASS.
- Product-scope projection and document freshness: PASS.
- Dependency-audit Rust cutover: PASS.
- Evidence-contract and source-regression checks: PASS.
- Replay-readiness Rust receipt and summary cutover: PASS.
- VM drift receipt Rust check: PASS.
- Local multi-hypervisor KVM smoke command and receipt schema: PASS.
- `nix flake check -L`: PASS, 40 flake checks and 486 workspace tests with 9 ignored.
- Cairn strict validation and proposal, design, and tasks gates: PASS.

The observed local KVM smoke campaign completed with both typed child plans timed out at their admitted 30-second limit. The campaign receipt correctly reported zero passed runs and `status=failed`. This is bounded timeout evidence, not a KVM success claim.

## Claim boundary

This migration improves ownership, explicit limits, and testability. It does not prove orchestration correctness, KVM behavior, sandboxing, complete audit coverage, release eligibility, or absence of defects.
