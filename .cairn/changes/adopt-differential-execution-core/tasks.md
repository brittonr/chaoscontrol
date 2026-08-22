# Tasks: Adopt differential-execution-core for the SpaceWasm differential rail

> Keep raw evidence under `.cairn/changes/adopt-differential-execution-core/evidence/` and summarize replay steps in `verification.md` at closeout.

## Phase 1: Pinned dependency and admission adapter

- [ ] [serial] T1.1 Add `differential-execution-core` to `crates/chaoscontrol-wasm-differential/Cargo.toml` at an immutable Git revision (the pinned Seaglass-consumed repository), and record the pin and lockfile change in the same commit. r[chaoscontrol.diff_exec_core.pin]
  - Trace: Proposal SC-01
  - Positive evidence: `cargo metadata`/`cargo tree` resolves the pinned revision; the crate compiles with the new dependency
  - Negative evidence: a dependency entry without an immutable revision fails resolution with a diagnostic
- [ ] [serial] T1.2 Add an admission adapter in `lib.rs` that maps the normalized SpaceWasm/Wasmtime engine observations onto the harness record with named nonzero bounds (events, canonical value bytes, set metadata), and exposes the admitted observations. r[chaoscontrol.diff_exec_core.bounds] r[chaoscontrol.diff_exec_core.case_identity]
  - Trace: Proposal SC-01, SC-04
  - Positive evidence: in-bounds engine observations admit under the case identity
  - Negative evidence: an observation exceeding a bound fails closed naming the violated bound
- [ ] [serial] T1.3 Add positive and negative fixture tests for the independence gate: two distinct engine implementations are admitted, a duplicate implementation fails with `DuplicateImplementation`, and a single backend fails with `OneBackend`. r[chaoscontrol.diff_exec_core.independence]
  - Trace: Proposal SC-03
  - Positive evidence: two independent engine observations admit
  - Negative evidence: duplicate- and single-backend admission both fail closed with the named error

## Phase 2: Comparison boundary and verdict preservation

- [ ] [serial] T2.1 Wire the per-case verdict through `admit_independent_backends` plus `compare_pairwise`, keeping the crate `CaseComparison`/`ComparisonVerdict` as the shell-facing projection, and record the harness `Identity` on each case. r[chaoscontrol.diff_exec_core.comparison] r[chaoscontrol.diff_exec_core.case_identity]
  - Trace: Proposal SC-02
  - Positive evidence: the rebuilt comparison produces the same per-case verdicts as the prior recorded transcript
  - Negative evidence: a forced divergence reports `Divergence` and exposes the first mismatch surface
- [ ] [serial] T2.2 Add a replay gate that replays the recorded prior corpus through the new boundary and asserts per-case verdict and report identity preservation. r[chaoscontrol.diff_exec_core.comparison]
  - Trace: Proposal SC-02
  - Evidence: the replay transcript matches the recorded baseline; drift fails with the first differing case named
- [ ] [serial] T2.3 Add oracle classification fixtures behind the existing fixture type: an exact-match external fixture classifies both observations as expected, and a fixture whose identity differs from the case fails closed naming the mismatch. r[chaoscontrol.diff_exec_core.oracle]
  - Trace: Proposal SC-04
  - Positive evidence: exact fixtures classify as exact match
  - Negative evidence: identity drift fails closed
- [ ] [serial] T2.4 Confirm the report projection still records the original normalized observations, per-case verdict, mismatch minimization, and the required non-claims, keeping `REPORT_SCHEMA` stable. r[chaoscontrol.diff_exec_core.report]
  - Trace: Proposal SC-02
  - Evidence: a completed run records the full observation, verdict, minimization, and non-claims

## Validation

- [ ] [serial] V1. Run the crate test suite (admission positive/negative, independence gate, comparison, oracle) and record the transcript. r[chaoscontrol.diff_exec_core.independence]
  - Raw evidence path: `evidence/adoption-fixtures.txt`
- [ ] [serial] V2. Replay the recorded corpus through the new boundary and record the verdict-preservation transcript. r[chaoscontrol.diff_exec_core.comparison]
  - Raw evidence path: `evidence/replay-preservation.txt`
- [ ] [serial] V3. Run the touched-target clippy with `-D warnings` and the existing differential rail regression, and record the transcript. r[chaoscontrol.diff_exec_core.pin]
  - Raw evidence path: `evidence/completion-gates.txt`
- [ ] [serial] V4. Run `cairn validate --root .`. r[chaoscontrol.diff_exec_core.report]
  - Raw evidence path: `evidence/cairn-validate.txt`

## Lifecycle / Closeout

- [ ] [serial] State whether this change is ready to archive or blocked by a named remaining item. r[chaoscontrol.diff_exec_core.report]
  - Remaining blocker or archive-ready note: pending implementation and evidence
  - Evidence: `verification.md`
