# Verification: Adopt differential-execution-core

## What was verified

`chaoscontrol-wasm-differential` now depends on `differential-execution-core`
at an immutable revision (`cad5a7cb6c6502b0ec5dc76c51e03352260d8efd`, the same
revision Seaglass pins). Every per-case verdict — in the crate fixtures and in
the live SpaceWasm differential rail — is computed through the reviewed
boundary: `admit_case`, bounded `admit_observation`,
`admit_independent_backends`, and `compare_pairwise`; `classify_with_oracle`
is exposed for exact-fixture attribution. The crate's own
`NormalizedObservation` / `CaseComparison` / `ComparisonVerdict` types remain
the shell-facing report projection, unchanged.

## Evidence

- `evidence/adoption-fixtures.txt` — crate test suite 12/12 green, including
  five adoption fixtures (two-engine admission, duplicate/single-backend
  fail-closed, bound case identity, agreement/divergence preservation,
  exact/drifted oracle classification).
- `evidence/replay-preservation.txt` — live nix rail
  `.#checks.x86_64-linux.spacewasm-mvp-differential` green after rewiring:
  cases=14 mismatches=0, report identity `770c5849...` byte-identical to the
  pre-rewiring run (verdict and report preservation at live-corpus level);
  resume equivalence ok (33 segments, streaming finished).
- `evidence/completion-gates.txt` — clippy `-D warnings` clean;
  `cargo check --workspace` clean; dependency pin and toolchain note.

## Toolchain change

DEH requires rustc 1.96.0+. The `rust-overlay` input in `flake.lock` moved to
the 2026-08-22 revision; dev and CI rustc are now 1.98.0. The workspace stays
green.

## Bundle pin refresh (pre-existing drift resolved)

The committed SpaceWasm profile pinned a Mantle bundle state that no longer
matched the pinned bundle input: expected manifest digest `39e4790a...` versus
actual `13058ea2...`. Verified pre-existing by reproducing the identical
failure on a worktree at `f9f1778` (before this change). Refreshed both pins
in the profile source (.ncl) and export (.json):

- `bundle_manifest_blake3`: `13058ea2d9913348a203cceff7b58d98b6446610ac80518dc3359b8d7ee57472`
- `bundle_identity_blake3`: `260f66f8df52b89f5673cbf3f2702d49d3413d45d47ab2baca994185d29e5cb3`

The host-runner member digest still matches the committed
`spacewasm_runner_blake3`, so no engine artifact changed — only the stale
bundle pins moved. With refreshed pins the full live rail passes.

## Boundaries

- Agreement through the harness remains parity only; no engine is attributed
  as correct without an explicit oracle fixture.
- The engine execution shells, generated corpus, and non-claims are unchanged.
