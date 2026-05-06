## Why

Persisted replay parent snapshots are implemented and contract-checked, but current workload evidence still exercises only schedule-only bugs (`replay_parent_depth = 0`). The next hardening slice needs a first-class workload/evidence rail that deliberately produces or imports a bug requiring parent context, proves its snapshot artifact is durable, and records standalone reproduce/minimize behavior against that artifact.

## What Changes

- Add a snapshot-backed workload evidence requirement for bug evidence with `replay_parent_depth > 0` and non-null `replay_parent_snapshot_ref`.
- Define an operator-facing campaign/export/reproduce path that uses normal artifacts rather than fixtures-only proof.
- Require receipts to distinguish positive replay proof, negative replay-gap evidence, and skipped/no-bug campaign attempts.
- Keep raw logs and voluminous checkpoints local unless summarized and hash-bound into concise evidence.

## Capabilities

### Modified Capabilities
- `replay-parent-snapshots`: Adds workload-level acceptance evidence for persisted parent snapshot replay beyond unit/contract fixtures.

## Impact

- **Files**: OpenSpec deltas now; implementation likely touches targeted campaign docs/scripts, evidence materialization/checking, and dogfood result receipts.
- **APIs**: No required public API change; CLI/script additions are allowed if they make the workload rail repeatable.
- **Dependencies**: No new dependency is expected.
- **Testing**: Strict OpenSpec validation now; implementation acceptance requires targeted Rust checks, evidence contract checks, Nix evidence check, and a curated dogfood/workload receipt.

## Out of Scope

- Making schedule-only bugs deterministic by assumption.
- Committing raw `run.log`, `reproduce.log`, temporary minimizer logs, or full voluminous checkpoints as acceptance evidence.
- Replacing public JSON/hash-addressed snapshot evidence with an opaque database-only artifact store.
