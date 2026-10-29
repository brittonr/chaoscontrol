## Why

Antithesis distinct_by_moment merges event sets while keeping one event per vtime, with earlier event sets taking precedence. ChaosControl Union deduplicates by source position, so it cannot prefer enriched observations while emitting only one row per retained simulation tick.

## What Changes

Add DistinctByTick { inputs } with one or more independently queried branches. Preserve input order as branch priority, first encountered row within each branch, and complete winning envelopes/projections. Keep one row per tick across all supplied sources; canonical sorting happens after winner selection. Admit every materialized branch row, including discarded collisions, and preserve existing limits/moments/receipts.

## Impact

Owner: replay/query maintainers. Consumer: operators producing compact retained-event timelines while preferring projected or higher-value observations. Durable capability: reusable priority-preserving time deduplication. This is simulation-tick identity, not upstream Moment/session/lineage equivalence or automatic per-VM/source partitioning. Existing queries/schemas and Union semantics remain unchanged; new readers are required. No hosted service, complete capture, VM execution or replay-proof claim.
