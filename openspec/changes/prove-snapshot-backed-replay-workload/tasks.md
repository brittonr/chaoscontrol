## Phase 1: Spec Foundation

- [x] [serial] Define workload-level snapshot-backed replay evidence requirements and keep implementation acceptance tasks open.

## Phase 2: Workload Selection

- [ ] [serial] Identify or add a bounded targeted workload/scenario that can produce a bug with `replay_parent_depth > 0` through the real explorer path.
- [ ] [parallel] Document the exact campaign/export command shape, including kernel/initrd derivation, seeds, budgets, and timeout/interruption handling.
- [ ] [parallel] Add or update operator docs to distinguish snapshot-backed proof, schedule-only replay gaps, no-bug runs, and missing-artifact skips.

## Phase 3: Evidence and Finalization Rail

- [ ] [serial] Ensure interrupted campaign finalization uses `chaoscontrol-explore export-bugs` or a checked wrapper and preserves replay parent snapshot refs.
- [ ] [parallel] Extend receipt materialization/checking if needed so snapshot-backed replay classification and artifact hashes are machine-checkable.
- [ ] [parallel] Add negative evidence handling for no-bug and schedule-only-only attempts without committing raw logs or huge checkpoints.

## Phase 4: Reproduce/Minimize Evidence

- [ ] [serial] Run the selected workload and retain at least one candidate bug with `replay_parent_depth > 0` and a valid `replay_parent_snapshot_ref`, or record a bounded negative coverage-gap receipt.
- [ ] [serial] Run standalone reproduce against the snapshot-backed candidate and save concise classified status evidence.
- [ ] [parallel] Run minimization only when reproduce semantics make the minimized artifact meaningful, and otherwise record why minimization was skipped or negative.

## Phase 5: Verification and Closeout

- [ ] [serial] Run targeted Rust tests for touched code plus `python scripts/check-evidence-contracts.py`, `python scripts/check-contract-registry.py`, `nix build .#checks.x86_64-linux.evidence-contracts --no-link -L`, and `git diff --check`.
- [ ] [serial] Commit/push curated evidence and implementation changes, then archive this OpenSpec only after every retained task has evidence.
