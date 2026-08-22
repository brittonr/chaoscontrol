# Proposal: Adopt differential-execution-core for the SpaceWasm differential rail

> Status: planning. Implementation and evidence pending. This change scopes the adoption; no crate dependency or comparison boundary changes are made yet.

## Why

`chaoscontrol-wasm-differential` already runs a genuine two-engine differential (SpaceWasm versus Wasmtime) over the admitted WebAssembly 1.0 core-module corpus and produces normalized observations, per-case comparisons, mismatch minimization, and non-claims. It does all of this with hand-rolled types: its own `NormalizedObservation`, `CaseComparison`, `ComparisonVerdict`, `FirstDifference`, a private `first_difference`, and its own report identity hashing.

`differential-execution-harness` (radicle `z2pbwQ4...`) exists precisely for this job and is now a reviewed public boundary. Its `no_std` functional core owns deterministic identity, case and observation admission with explicit bounds, pairwise comparison that reports a bounded `MismatchPath`, and oracle classification. Seaglass already wraps its TPC-H rail around this core; the SpaceWasm rail is the same shape with two independent WebAssembly execution backends.

The hand-rolled comparison duplicates real domain logic that the harness has already reviewed and sealed. Adopting the pinned core replaces that private `first_difference`/`ComparisonVerdict` machinery with the reviewed `admit_case` / `admit_observation` / `admit_independent_backends` / `compare_pairwise` / `classify_with_oracle` boundary, and binds each case to an explicit `Identity` like every other harness consumer. The existing SpaceWasm/Wasmtime execution shell, normalization of the two engine reports, bounds, minimization, and non-claims stay in ChaosControl.

## What Changes

- Add a pinned `differential-execution-core` dependency to `crates/chaoscontrol-wasm-differential` and keep it out of the normal-dependency graph of the VM shells.
- Replace the crate-private `CaseComparison`/`FirstDifference`/`first_difference` comparison path with `admit_independent_backends` plus `compare_pairwise`, and keep the existing report, minimization, and non-claims as the shell projection.
- Bind each admitted case and every observation to the harness `Identity`, admit observations under the existing bounds, and keep oracle classification available for cases with an exact external fixture.
- Keep `chaoscontrol` as the domain for requirement identifiers.

## Impact

- **Files**: `crates/chaoscontrol-wasm-differential/Cargo.toml`, `crates/chaoscontrol-wasm-differential/src/lib.rs`, `crates/chaoscontrol-wasm-differential/src/main.rs`, an admission/normalization module for the harness boundary, evidence under `.cairn/changes/adopt-differential-execution-core/evidence/`, and the change artifacts.
- **Testing**: positive and negative admission fixtures, independence-gate fixtures (single and duplicate backend), pairwise comparison fixtures against the current verdicts, oracle classification fixtures, and the existing differential rail regression.
- **Consumers**: ChaosControl differential operator, differential-execution-harness maintainers, release and Cairn operators.

## Success Criteria

1. **SC-01 — Pinned dependency and reviewed core boundary**
   - Verification scope or evidence destination: `differential-execution-core` is pinned to an immutable revision and the comparison path in `lib.rs` routes through the harness admission and pairwise functions.
2. **SC-02 — Existing verdicts are preserved**
   - Verification scope or evidence destination: the prior generated corpus still yields the same `Match`/`Mismatch` per-case verdicts when replayed through the new boundary; each report records the harness case identity.
3. **SC-03 — Independence gate is not vacuous**
   - Verification scope or evidence destination: a single-backend and a duplicate-backend observation both fail closed with the named independence error; positive fixtures admit two distinct backend implementations.
4. **SC-04 — Bounds and oracle behave**
   - Verification scope or evidence destination: admission rejects observations that exceed the configured bounds; oracle classification against an exact fixture is available and fails closed when the fixture identity does not match the case.
5. **SC-05 — Regression and lifecycle green**
   - Verification scope or evidence destination: the differential fixture tests and prior rail transcripts pass, clippy with `-D warnings` is clean on the touched crate, and `cairn validate --root .` passes at closeout.

## Expected Verification

- Replay command or audit: run the crate test suite (positive and negative admission, independence, comparison, oracle), replay the recorded corpus transcript, run the touched-target clippy, and `cairn validate --root .`.
- Raw evidence path: `.cairn/changes/adopt-differential-execution-core/evidence/`.
- What this check proves: the SpaceWasm differential rail runs through the reviewed harness core without changing its verified verdicts, and fails closed on independence and bound violations.
- Clean-baseline or audit-boundary note: agreement still establishes parity only; it does not prove correctness or attribute which engine is wrong.
- Gate or validation step: `cairn validate --root .` plus proposal, design, and tasks gates.

## Non-Goals

- No change to the SpaceWasm/Wasmtime execution shells, the existing corpus, or engine normalization logic beyond the comparison boundary.
- No new second consumer in this change; Seaglass stays the reference integration.
- No attempt to prove WebAssembly equivalence or to replace the diagnostic-only status of the rail.
- No network access at test time beyond the already-pinned corpus and report material.
