# Design: Adopt differential-execution-core for the SpaceWasm differential rail

## Context

`chaoscontrol-wasm-differential` produces a real two-engine differential over the admitted WebAssembly 1.0 core-module corpus: pure `lib.rs` logic generates and normalizes observations, compares SpaceWasm and Wasmtime normalized outcomes, minimizes mismatches, and hashes receipt identity; the `main.rs` shell runs both engine binaries through Bounded Exec under typed bounds and reports. The private comparison logic reimplements what `differential-execution-core` already reviews and seals: observation admission with bounds, independent-backend admission, a bounded mismatch surface, pairwise comparison, and oracle classification.

`differential-execution-core` is the reviewed shared boundary (radicle `z2pbwQ4...`). Seaglass consumes it for its TPC-H rail; this change adopts it for the SpaceWasm rail. The adoption must not disturb the two execution shells, the corpus, or the engine normalization, and must preserve the prior per-case verdicts.

## Decisions

### Decision: Keep the execution shells, replace only the comparison boundary

**Choice:** Keep `normalize_spacewasm`, `normalize_tool`, `compare_case`'s inputs, the fixture generation, chunk schedules, engine execution in `main.rs`, and the report projection. Replace the comparison result path so that `admit_independent_backends` plus `compare_pairwise` (and `classify_with_oracle` where a fixture exists) produce the per-case verdict, with the crate's `CaseComparison`/`ComparisonVerdict` kept as the shell-facing projection.

**Rationale:** Diff budget is engine execution and corpus maintenance, not the reviewed seam. Replacing the seam is a small, high-value win: it removes duplicated identity/admission logic and binds the rail to the harness boundary that downstream consumers already trust. The shell keeps responsibility for process, file, and tool effects.

**Risk:** the harness observation model (events, value bytes, resource outcome, final state identity) differs from the current flat `NormalizedObservation`. Mitigation: keep an explicit admission adapter in `lib.rs` that maps the normalized engine observations onto the harness record with named bounds, and keep the existing flat type only at the report projection if operators require it.

### Decision: Pin to an immutable revision and keep out of VM shells

**Choice:** Add `differential-execution-core` to `crates/chaoscontrol-wasm-differential` at a fixed immutable Git revision (same repository and pin strategy Seaglass uses). Do not add it to the `no_std` VM shell dependency graphs.

**Rationale:** An immutable pin makes the reviewed seam reproducible; keeping it out of the VM shells avoids License/authority coupling outside the differential crate. `no_std` consumers of the harness core already link the same crate.

### Decision: Preserve the verified verdicts with a replay gate

**Choice:** Record the prior per-case `Match`/`Mismatch` verdicts (and their report identity) as a fixture, and require the replayed corpus through the new boundary to match them. Do not silently change what a case means while porting.

**Rationale:** The existing archived rail is the baseline. A replay gate proves the adoption did not alter the differential result; divergence after the port would indicate a real bug in the adapter, not a review artifact.

**Risk:** engine or corpus drift after the archive could produce a false failure. Mitigation: the gate replays the recorded corpus/report material directly, not a network fetch, and documents any admitted version change.

### Decision: Oracle classification is available, not required

**Choice:** Wire `classify_with_oracle` behind the existing `OracleFixture` type for cases that carry an exact external fixture. Keep raw diagnostic runs fixture-less (as today), and keep `Unattributed` as the honest default when no oracle exists.

**Rationale:** The SpaceWasm rail is diagnostic-only; it does not claim correctness attribution. Adding the seam without forcing a fixture preserves the current non-claims while making attribution reachable where an operator supplies a fixture.

## Risks / Trade-offs

- Mapping the flattened `NormalizedObservation` onto the harness `NormalizedObservation` risks losing an engine-specific field. Mitigation: every field is admitted under named bounds, and the report retains the full original observation for diagnosis.
- The existing report schema must not be forced to change for operators that already consume it. Mitigation: keep `REPORT_SCHEMA` and the report fields stable; the harness identity appears as an added field inside the case record.
- `differential-execution-core` is published from a seed-served revision; resolution must stay reproducible. Mitigation: immutable pin and `Cargo.lock` update in the same commit as the code change.
