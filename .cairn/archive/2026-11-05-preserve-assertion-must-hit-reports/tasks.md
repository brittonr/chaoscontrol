## 1. Foundation

- [x] [serial] 1.1 Record the report metadata contract and owned consumers. r[chaoscontrol.assertion_reports.must_hit]

## 2. Implementation

- [x] [parallel] 2.1 Preserve must-hit metadata through report projections and constructors. r[chaoscontrol.assertion_reports.must_hit]
- [x] [parallel] 2.2 Align summary verdict recomputation and Rust/Nickel evidence admission. r[chaoscontrol.assertion_reports.must_hit_verdict]
- [x] [serial] 2.3 Reject campaign metadata conflicts and test cross-seed verdict recomputation. r[chaoscontrol.assertion_reports.must_hit_conflict]

## 3. Verification

- [x] [serial] 3.1 Exercise oracle-to-summary output, positive/negative regressions, focused checks and native lifecycle gates; update the parity reference with scope and evidence. r[chaoscontrol.assertion_reports.must_hit_verdict]

### Observed evidence (2026-10-28)

- Baseline: `cargo test -p chaoscontrol-explore --test assertion_summary_v2` (6 passed) and `cargo test -p chaoscontrol-evidence --test assertion_summary_identity` (13 passed).
- Regression failed before the fix: `zero_hit_must_hit_summaries_preserve_oracle_verdicts` rejected `must_hit` as an unknown field.
- `cargo test -p chaoscontrol-explore -p chaoscontrol-evidence --all-targets`: 1137 passed, 2 ignored, 90 suites.
- Throwaway `must_hit_report_smoke` example executed real catalog activation, metadata ingestion, strict projection, summary readback and promotion validation. Output: `must-run: must_hit=Some(true), hits=0, verdict=failed`; `may-skip: must_hit=Some(false), hits=0, verdict=passed`; forged unexercised verdict rejected by both readers. The example was removed after execution.
- `cargo run -q -p chaoscontrol-evidence --bin check-evidence-contracts -- --root .`: passed including Nickel positive/negative fixtures. Initial product-scope admission failure was corrected by registering this change and regenerating its owned projections.
- `cargo check --workspace --all-targets` and `cargo clippy -p chaoscontrol-explore -p chaoscontrol-evidence --all-targets -- -D warnings`: passed.
- Cargo commands used `TMPDIR=/home/brittonr/.cargo-target/tmp`. kache reported cache I/O errors and compiled uncached; commands succeeded.
- Global `cairn validate --root . --strict` failed on the unrelated pre-existing `.cairn/changes/add-native-process-record-replay-profile/tasks.md:17` missing known requirement reference. This change does not repair or relabel that failure.
- Evidence class: pure local reporting/instrumentation, based on source revision `986a59c7` plus this change; no guest boot, KVM campaign, replay or release claim. Maintenance owner: ChaosControl assertion/report maintainers.
- Final selected proposal/design/tasks gates and `cargo fmt --all -- --check` passed. Requirements were synchronized through native Cairn after verification; the change remains active and unarchived.
- Final `cairn traceability coverage --root . --json`: 396 accepted requirements, no missing requirements; fails on nine unrelated existing native-process-profile and SMR-storage-recovery dangling IDs. All three report must-hit IDs are recognized and linked. Earlier traceability attempts exposed this change's trailing requirement markers; placing them at the start of normative paragraphs and resynchronizing corrected those own links.
