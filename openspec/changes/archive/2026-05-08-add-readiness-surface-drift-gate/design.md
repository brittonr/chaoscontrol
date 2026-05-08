## Context

The replay-readiness shell records static gate statuses and emits a receipt, while sibling scripts render summaries, dashboards, and README snippets from receipts. Adding a new static gate requires updating both the executable run list and receipt gate list. A missing receipt entry weakens operator trust even if the gate ran.

## Goals / Non-Goals

**Goals:**
- Detect mismatch between replay-readiness `run_gate` entries and receipt `static_gates` entries.
- Prove summary/dashboard/README rendering use the same summary line and bounded-scope language.
- Keep the check cheap and KVM-free.

**Non-Goals:**
- Validate snapshot artifacts or replay verdicts; existing gates own those.
- Commit generated dashboard or receipt artifacts.
- Change dashboard styling.

## Decisions

### 1. Source-level gate list comparison

**Choice:** Parse the replay-readiness block in `flake.nix` and compare `run_gate` names against receipt tuple names.
**Rationale:** This catches omissions before any CI artifact is published and avoids depending on Nix build output paths.
**Alternative:** Inspect only generated receipt artifacts; rejected because local commits may not have a receipt artifact yet.

### 2. Renderer consistency fixture

**Choice:** Use the dashboard sample receipt as the single fixture for summary, dashboard, and README update checks.
**Rationale:** This verifies the operator surfaces share the same summary line and bounded scope text without duplicating receipt construction.

## Risks / Trade-offs

**Regex brittleness** → Bound parsing to stable replay-readiness markers and fail with clear messages if the structure changes.

**Self-reference** → Include the drift checker itself in both the run list and receipt list so future additions follow the same rule.
