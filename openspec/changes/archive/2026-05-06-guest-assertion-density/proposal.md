## Why

ChaosControl already has the hard part of assertion infrastructure: compile-time catalogs, unexercised-site reporting, and campaign aggregation. What it lacks is guest-side density. The current guests still concentrate assertions at a few top-level invariants, which leaves large semantic blind spots where the explorer can reach interesting states without a property to evaluate.

## What Changes

- Define a guest assertion density capability that treats assertion placement as part of guest design, not an afterthought.
- Expand the Raft guest from its current safety/liveness core into denser transition, branch-pair, and recovery-path assertions.
- Add a redb-specific assertion spec covering write, read, delete, compaction, savepoint, crash-recovery, and durability boundaries against the shadow oracle.
- Extend campaign output with explicit assertion exercise targets so dense catalogs become actionable instead of passive telemetry.

## Capabilities

### New Capabilities
- `guest-assertion-density`: Per-guest rules for assertion placement, density review, and assertion exercise targets in exploration reports.
- `redb-guest-assertions`: Storage-oriented assertions for the redb guest, covering committed-state preservation, recovery behavior, and operation-level oracle checks.

### Modified Capabilities
- `raft-assertions-v2`: Extend Raft assertions beyond the existing safety/liveness set to cover state transitions, paired branch outcomes, and fault/recovery evidence.
- `campaign-runner`: Make assertion exercise ratios first-class campaign output, with an optional floor for automation and CI.

## Impact

- **Crates**: `chaoscontrol-raft-guest`, `chaoscontrol-redb-guest`, `chaoscontrol-explore`, and small supporting changes in `chaoscontrol-sdk` if helper macros are needed.
- **Reports/CLI**: Campaign and run reports gain explicit assertion exercise summaries and optional gating.
- **Docs**: `docs/assertion-guidelines.md` and guest-specific notes will need to reflect the new density model.
- **Testing**: New guest tests for assertion presence/exercise and campaign-level tests for assertion exercise aggregation.
- **No breaking API changes.** This is additive guest instrumentation plus report/CLI extensions.
