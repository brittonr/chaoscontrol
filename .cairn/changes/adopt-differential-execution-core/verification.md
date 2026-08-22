# Verification: Adopt differential-execution-core

## What was verified

The `chaoscontrol-wasm-differential` crate now depends on
`differential-execution-core` at an immutable revision
(`cad5a7cb6c6502b0ec5dc76c51e03352260d8efd`, the same revision Seaglass pins) and
routes case admission, observation admission, independence, pairwise
comparison, and oracle classification through that reviewed boundary. The
crate's own `NormalizedObservation` / `CaseComparison` / `ComparisonVerdict`
types remain the shell-facing report projection, unchanged.

## Evidence

- `evidence/adoption-fixtures.txt` — the crate test suite: 12/12 green, including
  five new adoption fixtures (two-engine admission, duplicate/single-backend
  fail-closed, bound case identity, agreement/divergence preservation, and
  exact/drifted oracle classification).
- `evidence/completion-gates.txt` — clippy `--all-targets -- -D warnings` clean;
  `cargo check --workspace` clean; dependency pin and toolchain note.

## Toolchain change

DEH requires rustc 1.96.0+. ChaosControl's dev toolchain was pinned to
`rust-bin.stable.latest` at 1.93.1. The `rust-overlay` input in `flake.lock`
was updated to the 2026-08-22 revision, moving the dev and CI rustc to 1.98.0.
`cargo check --workspace` stays green after the bump.

## Named blocker (not introduced by this change)

The live nix rail `nix build .#checks.x86_64-linux.spacewasm-mvp-differential`
fails on a bundle digest drift: the committed SpaceWasm profile pins
`bundle_manifest_blake3 = 39e4790a...`, while the current Mantle bundle hash is
`13058ea2...`. This predates this adoption and blocks the full live-corpus
replay (task V2). Fixture-level verdict preservation is verified instead; the
live replay will pass once that pre-existing profile/bundle pin is reconciled.

## Boundaries

- Adoption of the comparison seam does not change the engine execution shells,
  the generated corpus, or the non-claims.
- Agreement through the harness remains parity only; no engine is attributed
  as correct without an explicit oracle fixture.
