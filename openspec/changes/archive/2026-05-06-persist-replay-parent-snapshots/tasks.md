## Phase 1: Spec Foundation

- [x] [serial] Define the replay parent snapshot artifact boundary, redb role, public evidence shape, and verification plan.

## Phase 2: Snapshot Store Foundation

- [x] [serial] Add a Rust-owned snapshot artifact envelope with codec/schema version, digest, and replay-parent metadata.
- [x] [serial] Add a `SnapshotStore` trait with `put_snapshot`, `get_snapshot`, `has_snapshot`, and retention/GC hooks.
- [x] [parallel] Implement the baseline file-backed content-addressed store under run output directories.
- [x] [parallel] Add an optional host-side redb-backed store or index behind an explicit feature/module name that cannot be confused with the redb guest workload.

## Phase 3: Evidence Integration

- [x] [serial] Extend bug and receipt JSON with validated `replay_parent_snapshot_ref` fields while preserving backward compatibility for schedule-only bugs.
- [x] [serial] Update Nickel evidence contracts, registry entries, positive fixtures, and negative fixtures for valid, missing, corrupt, wrong-hash, unsupported-codec, incompatible-schema, and outside-store/path-escape snapshot refs.
- [x] [parallel] Update dogfood receipt materialization and README/operator docs to describe snapshot artifacts, redb's optional role, and raw-log exclusion boundaries.

## Phase 4: Replay Integration

- [x] [serial] Teach standalone replay/minimization to load parent snapshots from the snapshot store before executing schedules that require parent context.
- [x] [serial] Fail early with actionable diagnostics when required parent snapshots are absent or invalid rather than silently falling back to schedule-only replay.
- [x] [parallel] Add unit tests for store round trips, digest validation, and corruption diagnostics.
- [x] [parallel] Add a dogfood/replay fixture proving a saved bug can restore from its persisted parent snapshot artifact.

## Phase 5: Verification and Archive

- [x] [serial] Run targeted Rust tests, evidence contract checks, the Nix evidence-contracts check, and `git diff --check`.
- [x] [serial] Mark implementation tasks complete only after evidence exists, then archive the OpenSpec and validate the touched canonical specs.
