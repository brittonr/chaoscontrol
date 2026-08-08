## Context

ChaosControl already has compile-time assertion catalogs, unexercised assertion reporting, and campaign-level aggregation. The remaining gap is semantic density inside guests. The current Raft and redb guests expose useful top-level checks, but the catalog still says little about which operation families, recovery paths, or branch outcomes were actually exercised.

The change cuts across the SDK catalog path, guest crates, and explorer reporting. If density is going to become a design discipline instead of a one-off cleanup, the metadata has to survive all the way from macro expansion to campaign reports.

## Goals / Non-Goals

**Goals:**
- Add structured assertion-density metadata that can survive compile-time catalog registration and report aggregation.
- Make guest reports show assertion exercise in terms that are useful to humans: guest name, category, exercised ratio, and optional automation floor.
- Densify the Raft guest around transitions, branch pairs, and replication bookkeeping.
- Densify the redb guest around operation families, durability boundaries, repair, and recovery against the shadow oracle.

**Non-Goals:**
- Auto-generating assertions from code structure.
- Changing the semantics of `always`, `sometimes`, `reachable`, or `unreachable`.
- Retrofitting every historical guest in one pass; this change only needs the active guests that already matter for exploration.
- Making assertion floors mandatory by default. The gate should be opt-in until the catalogs stabilize.

## Decisions

### 1. Extend catalog entries instead of inferring density from message strings

`CatalogEntry` and the VMM/oracle/report pipeline will grow explicit density metadata: guest name plus category (`invariant`, `branch`, `operation`, `recovery`). This keeps unexercised assertions classifiable, which runtime JSON details alone cannot do.

**Rationale:** unexercised assertions only exist in the catalog, so any density summary must be available before the assertion ever fires.

**Alternative considered:** encode category in the assertion message prefix. Rejected because it is brittle, unreadable, and hard to validate.

### 2. Keep the SDK surface small; let guests define thin local wrappers

The SDK should expose one metadata-aware assertion registration path, but guest crates should define tiny local wrappers or helper macros for their own `guest` tag and the small number of categories they need.

**Rationale:** this avoids multiplying the public SDK API while still making callsites readable (`raft_branch_assert!`, `redb_recovery_assert!`, etc.).

**Alternative considered:** add a large matrix of new public macros for every category and assertion kind. Rejected because it bloats the API and makes future categories expensive.

### 3. Densify at semantic boundaries, not by sprinkling extra top-level checks

The new assertions will be added where meaning changes: operation entry, success/failure branches, state transitions, recovery boundaries, and mutation sites that update durable or replicated state.

**Rationale:** assertions are most valuable when they pin down the moment the mental model changes. This matches the TigerBeetle-style motivation without turning the catalog into noise.

**Alternative considered:** only add more post-tick summary assertions. Rejected because they miss the branch or mutation that caused the state.

### 4. Evaluate assertion exercise floors after writing reports

The optional `--min-assertion-exercise` gate will be checked after report generation. If the floor is missed, the command exits with a distinct non-success code while keeping the report artifacts on disk.

**Rationale:** CI needs artifacts even when the density floor fails.

**Alternative considered:** abort early when the floor becomes impossible to meet. Rejected because exploration should still finish and produce diagnostic output.

## Risks / Trade-offs

- **[Catalog churn]** -> Extending assertion metadata touches SDK, fault engine, report structs, and serialization. Mitigation: default legacy assertions to `uncategorized` and keep fields backward-compatible with `#[serde(default)]`.
- **[Overtight assertions]** -> Densifying guests can create false positives if branch expectations are too strong. Mitigation: focus on local invariants and paired reachability before adding more semantic liveness checks.
- **[Noisy reports]** -> Category summaries can overwhelm short reports. Mitigation: keep the summary compact and preserve the detailed table below it.
- **[Floor brittleness]** -> A hard threshold can flap while the guest catalog is still evolving. Mitigation: make the floor opt-in and document recommended starting values.

## Migration Plan

1. Extend SDK catalog metadata, oracle records, and report structs in a backward-compatible way.
2. Add report summaries and the optional exercise-floor CLI gate.
3. Migrate the Raft guest to category-aware helpers and densify its branch/transition assertions.
4. Migrate the redb guest to category-aware helpers and densify operation/recovery assertions.
5. Update assertion guidance docs and add coverage-style tests for the new catalog/report behavior.

## Open Questions

- Should the first version support only an overall floor, or should per-category floors follow immediately?
- Do we want a shared guest helper module for category wrappers, or should each guest crate keep its own local wrappers until the pattern stabilizes?
