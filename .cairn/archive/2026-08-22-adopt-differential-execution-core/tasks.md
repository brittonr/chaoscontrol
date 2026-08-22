# Tasks: Adopt differential-execution-core for the SpaceWasm differential rail

> Keep raw evidence under `.cairn/changes/adopt-differential-execution-core/evidence/` and summarize replay steps in `verification.md` at closeout.

## Phase 1: Pinned dependency and admission adapter

- [x] [serial] T1.1 Add `differential-execution-core` to `crates/chaoscontrol-wasm-differential/Cargo.toml` at an immutable Git revision (the pinned Seaglass-consumed repository), and record the pin and lockfile change in the same commit. r[chaoscontrol.diff_exec_core.pin]
  - Trace: Proposal SC-01
  - Positive evidence: `cargo tree` resolves the pinned revision; the crate compiles with the new dependency (rustc 1.98.0 after rust-overlay 2026-08-22 update)
  - Negative evidence: the dependency requires rustc 1.96.0+; the pre-bump dev toolchain (1.93.1) rejected it fail-closed until the rust-overlay pin was updated
- [x] [serial] T1.2 Add an admission adapter in `lib.rs` that maps the normalized SpaceWasm/Wasmtime engine observations onto the harness record with named nonzero bounds (events, canonical value bytes, set metadata), and exposes the admitted observations. r[chaoscontrol.diff_exec_core.bounds] r[chaoscontrol.diff_exec_core.case_identity]
  - Trace: Proposal SC-01, SC-04
  - Positive evidence: `admit_diff_case`/`admit_diff_observation` admit engine observations under the case identity
  - Negative evidence: omitted; bounds rejection covered by the harness `admit_observation` contract and existing crate bounds
- [x] [serial] T1.3 Add positive and negative fixture tests for the independence gate: two distinct engine implementations are admitted, a duplicate implementation fails with `DuplicateImplementation`, and a single backend fails with `OneBackend`. r[chaoscontrol.diff_exec_core.independence]
  - Trace: Proposal SC-03
  - Positive evidence: `harness_independence_gate_admits_two_distinct_engines` passes
  - Negative evidence: `harness_independence_gate_rejects_duplicate_and_single_backend` passes

## Phase 2: Comparison boundary and verdict preservation

- [x] [serial] T2.1 Wire the per-case verdict through `admit_independent_backends` plus `compare_pairwise`, keeping the crate `CaseComparison`/`ComparisonVerdict` as the shell-facing projection, and record the harness `Identity` on each case. r[chaoscontrol.diff_exec_core.comparison] r[chaoscontrol.diff_exec_core.case_identity]
  - Trace: Proposal SC-02
  - Positive evidence: `compare_case` now computes its verdict through the harness boundary; `harness_verdicts_preserve_prior_agreement_and_divergence` reports Agreement/Divergence through it
  - Negative evidence: the forced divergence fixture reports `Divergence` and exposes the first mismatch surface
- [x] [serial] T2.2 Add a replay gate that replays the recorded prior corpus through the new boundary and asserts per-case verdict and report identity preservation. r[chaoscontrol.diff_exec_core.comparison]
  - Trace: Proposal SC-02
  - Evidence: live nix rail GREEN after rewiring — cases=14 mismatches=0, report identity `770c5849...` byte-identical to the pre-rewiring run; the stale bundle pin was refreshed (`13058ea2...`/`260f66f8...`) after verifying the drift reproduced on the pre-change tree
- [x] [serial] T2.3 Add oracle classification fixtures behind the existing fixture type: an exact-match external fixture classifies both observations as expected, and a fixture whose identity differs from the case fails closed naming the mismatch. r[chaoscontrol.diff_exec_core.oracle]
  - Trace: Proposal SC-04
  - Positive evidence: `harness_oracle_classifies_exact_and_drifted_fixtures` passes with ExactMatch on both sides
  - Negative evidence: the drifted fixture reports `Mismatch` on the right side
- [x] [serial] T2.4 Confirm the report projection still records the original normalized observations, per-case verdict, mismatch minimization, and the required non-claims, keeping `REPORT_SCHEMA` stable. r[chaoscontrol.diff_exec_core.report]
  - Trace: Proposal SC-02
  - Evidence: the crate keeps its own `NormalizedObservation`/`CaseComparison` types; the DEH adapter is the only new surface and the report types are untouched

## Validation

- [x] [serial] V1. Run the crate test suite (admission positive/negative, independence gate, comparison, oracle) and record the transcript. r[chaoscontrol.diff_exec_core.independence]
  - Raw evidence path: `evidence/adoption-fixtures.txt` - 12/12 green
- [x] [serial] V2. Replay the recorded corpus through the new boundary and record the verdict-preservation transcript. r[chaoscontrol.diff_exec_core.comparison]
  - Raw evidence path: `evidence/replay-preservation.txt` - live rail green: 14 cases, 0 mismatches, report identity preserved
- [x] [serial] V3. Run the touched-target clippy with `-D warnings` and the existing differential rail regression, and record the transcript. r[chaoscontrol.diff_exec_core.pin]
  - Raw evidence path: `evidence/completion-gates.txt` - clippy clean; workspace check green
- [x] [serial] V4. Run `cairn validate --root .`. r[chaoscontrol.diff_exec_core.report]
  - Raw evidence path: `evidence/completion-gates.txt`

## Lifecycle / Closeout

- [x] [serial] State whether this change is ready to archive or blocked by a named remaining item. r[chaoscontrol.diff_exec_core.report]
  - Remaining blocker or archive-ready note: archive-ready — implementation, unit fixtures, live-corpus replay (14/14 match, report identity preserved), clippy, workspace check, and cairn validate all green
  - Evidence: `verification.md`
