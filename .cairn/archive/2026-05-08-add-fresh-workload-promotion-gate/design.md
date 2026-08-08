## Context

`docs/replay-readiness-status.md` is generated from `dogfood-results/accepted-workload-proofs.json` and currently labels fresh workload authoring as experimental until a workload has committed accepted snapshot-backed replay evidence. Existing checks validate the proof artifacts and report freshness, but there is no separate promotion-boundary check for the report/manifest relationship and anti-overclaim classifications.

## Goals / Non-Goals

**Goals:**
- Keep fresh workload promotion tied to accepted manifest entries and generated report rows.
- Fail closed when anti-claims or experimental/unproven classifications disappear.
- Provide cheap local negative fixtures without KVM.

**Non-Goals:**
- Run a new VM dogfood campaign.
- Promote arbitrary workloads or full Antithesis-style parity claims.
- Change replay verdict schema semantics.

## Decisions

### 1. Python promotion checker

**Choice:** Add a small Python checker over the accepted manifest and committed readiness report.
**Rationale:** The boundary is document/manifest consistency, not Rust runtime behavior. Python matches the surrounding readiness scripts and avoids KVM cost.
**Alternative:** Fold this into `check-replay-proof-coverage.py`; rejected because artifact proof validation and operator promotion classification are distinct failure modes.

### 2. Self-test negative fixtures

**Choice:** Include `--selftest` cases for duplicate assertion IDs, missing anti-claims, missing fresh-workload experimental classification, and unsupported report-only workload promotion.
**Rationale:** These catch overclaiming regressions without constructing large snapshot artifacts.

## Risks / Trade-offs

**Report parser brittleness** → Keep parsing narrow to the generated Markdown tables and fail with actionable messages.

**Duplicate gate cost** → The new check complements existing proof coverage/report freshness checks by validating cross-surface promotion semantics only.
