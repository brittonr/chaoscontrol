## Context

`assertions.json` files are Rust-owned runtime evidence and should not be hand-edited. The readiness report, however, can derive presentation metadata from stable assertion labels so operators can distinguish operation, invariant, replay-probe, and workload-driver gaps.

## Goals / Non-Goals

**Goals:** infer effective categories for accepted-proof summaries that omit them; surface the category source in gap details; keep deterministic checks and promotion gates intact.

**Non-Goals:** rerun VM campaigns, rewrite committed assertion artifacts, or claim zero coverage/non-passing gaps merely because categories were inferred.

## Decisions

### 1. Report-local effective category

**Choice:** add a small deterministic category resolver in `chaoscontrol-evidence` keyed by workload/message for accepted assertions whose category is missing or `uncategorized`.

**Rationale:** it provides immediate operator signal from committed artifacts without mutating runtime evidence.

**Alternative:** patch `assertions.json`; rejected because runtime-emitted evidence must remain Rust-owned and unhand-edited.

### 2. Source-aware gap detail

**Choice:** render `category=...` for artifact categories and `category=... (inferred)` for inferred fallback categories.

**Rationale:** this prevents overclaiming that legacy artifacts carried metadata they did not actually contain.

## Risks / Trade-offs

**Stale mappings** → keep mappings small and covered by tests; unknown labels remain `uncategorized` and continue to block promotion.
