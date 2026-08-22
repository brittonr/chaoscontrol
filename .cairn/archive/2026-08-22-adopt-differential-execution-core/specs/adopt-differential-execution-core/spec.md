# Adopt differential-execution-core Specification

## Purpose

Bind the SpaceWasm differential rail to the reviewed `differential-execution-core` boundary so comparison, admission, identity, independence, and oracle classification come from the pinned shared core instead of crate-private logic.

## ADDED Requirements

### Requirement: Pinned core dependency

r[chaoscontrol.diff_exec_core.pin] The `chaoscontrol-wasm-differential` crate MUST depend on `differential-execution-core` at a pinned immutable revision, and MUST route the pairwise comparison through `admit_independent_backends` and `compare_pairwise`.

#### Scenario: Pinned revision resolves
- GIVEN the declared dependency on `differential-execution-core`
- WHEN the workspace resolves the dependency graph
- THEN the pinned immutable revision MUST resolve
- AND the comparison boundary MUST be reachable from the crate.

#### Scenario: Missing pin fails
- GIVEN a dependency entry without an immutable revision
- WHEN the dependency is resolved
- THEN resolution MUST fail closed with a diagnostic.

### Requirement: Admitted cases carry an identity

r[chaoscontrol.diff_exec_core.case_identity] Any case admitted through the core MUST be admitted with `admit_case`, and every observation MUST carry the case identity produced by that admission.

#### Scenario: Case admission succeeds
- GIVEN a well-formed corpus module case
- WHEN the case is admitted
- THEN admission MUST succeed
- AND the reported case identity MUST be a 64-hex-character digest.

#### Scenario: Malformed admission fails
- GIVEN a case whose profiles cannot be admitted under the configured bounds
- WHEN the case is admitted
- THEN admission MUST fail closed with the violating profile named.

### Requirement: Independent backend gate

r[chaoscontrol.diff_exec_core.independence] Before any pairwise comparison, the two observations MUST be admitted as independent backends with `admit_independent_backends`, and a single- or duplicate-backend pair MUST fail closed.

#### Scenario: Two distinct engines admitted
- GIVEN one SpaceWasm observation and one Wasmtime observation for the same case
- WHEN `admit_independent_backends` runs
- THEN it MUST accept the pair.

#### Scenario: Duplicate backend fails closed
- GIVEN two observations that name the same backend implementation
- WHEN `admit_independent_backends` runs
- THEN it MUST fail with the duplicate-implementation error
- AND MUST NOT report agreement.

#### Scenario: Single backend fails closed
- GIVEN only one observation for the case
- WHEN `admit_independent_backends` runs
- THEN it MUST fail with the one-backend error.

### Requirement: Bounded observation admission

r[chaoscontrol.diff_exec_core.bounds] Every observation MUST be admitted under explicit nonzero bounds for events, canonical value bytes, and set metadata, and an observation that exceeds a bound MUST fail closed.

#### Scenario: In-bounds observation admitted
- GIVEN an observation within the configured bounds
- WHEN it is admitted
- THEN admission MUST succeed.

#### Scenario: Bound violation fails
- GIVEN an observation that exceeds a configured bound
- WHEN it is admitted
- THEN admission MUST fail closed
- AND the failure MUST name the violated bound.

### Requirement: Pairwise comparison and preservation

r[chaoscontrol.diff_exec_core.comparison] The per-case verdict MUST come from `compare_pairwise`, and the replayed corpus MUST preserve the prior per-case `Match`/`Mismatch` verdicts.

#### Scenario: Verdicts preserved on replay
- GIVEN the recorded prior corpus-derived observations
- WHEN the new boundary compares each case
- THEN each per-case verdict MUST equal the prior recorded verdict.

#### Scenario: Divergence names a surface
- GIVEN a forced divergence between the two engine observations
- WHEN `compare_pairwise` runs
- THEN the report MUST report `Divergence`
- AND MUST expose the first bounded mismatch surface.

### Requirement: Oracle classification available and fail-closed

r[chaoscontrol.diff_exec_core.oracle] For a case with an exact external fixture, `classify_with_oracle` MUST classify both observations, and MUST fail closed when the fixture identities do not match the case.

#### Scenario: Exact-match oracle
- GIVEN an external oracle fixture bound to the case identity
- WHEN both observations are classified
- THEN the classification MUST be recorded
- AND an exact match MUST classify as such.

#### Scenario: Fixture identity drift fails
- GIVEN an oracle fixture whose identity differs from the admitted case
- WHEN classification runs
- THEN it MUST fail closed naming the identity mismatch.

## Modified Requirements

### Requirement: Report projection retains non-claims

r[chaoscontrol.diff_exec_core.report] The existing differential report MUST continue to record the engine-normalized observations, per-case verdicts, mismatch minimization, and the required non-claims as a shell projection over the harness boundary.

#### Scenario: Report still records observations
- GIVEN a completed differential run
- WHEN the report is produced
- THEN it MUST still record the SpaceWasm and Wasmtime observations, verdict, minimization, and non-claims.

## Unchanged Requirements

- `r[chaoscontrol.spacewasm_mvp.functional_core]` the pure admission, generation, normalization, comparison, and receipt identity logic stays in `lib.rs`.
- `r[chaoscontrol.spacewasm_mvp.differential]` the rail remains a genuine two-engine differential over the admitted WebAssembly 1.0 core-module corpus.
- `r[chaoscontrol.spacewasm_mvp.evidence]` evidence remains diagnostic-only; agreement is parity, not correctness.
