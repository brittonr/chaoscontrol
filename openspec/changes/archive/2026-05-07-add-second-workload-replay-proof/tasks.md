## Phase 1: Spec

- [x] Define second-workload snapshot-backed replay evidence requirement.

## Phase 2: Implementation

- [x] Add or select a bounded non-Raft snapshot replay probe workload.
- [x] Parameterize the accepted verdict dogfood wrapper for the non-Raft workload.

## Phase 3: Evidence

- [x] Run the non-Raft workload through filtered export and standalone reproduce.
- [x] Curate concise accepted evidence with artifact hashes and no raw logs/checkpoints.

## Phase 4: Verification

- [x] Run focused Rust/script/evidence checks.
- [x] Commit, push, and archive the OpenSpec after evidence exists.
