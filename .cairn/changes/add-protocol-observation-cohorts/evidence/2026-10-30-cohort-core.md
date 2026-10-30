# Cohort core evidence

Date: 2026-10-30. Source: worktree `cairn/molten-contracts-20261029` on base `1caeda2334f1d93e53ce22afbfa012b05458f606`, run in `nix develop` with build, temp, and cache directories under `/home/brittonr/.cargo-target/`.

## Scope

This evidence covers the pure domain types, canonical envelope admission with domain-separated BLAKE3 identities, cohort assembly by exact profile and boundary refs, the complete/incomplete/conflicting/unsupported classification, and stable novelty identities in `chaoscontrol_smr::observation`. It also covers the `MultiNodeGuestIntake` hook and `validate_guest_history`, which hand admitted cohorts to the existing independent SMR history checker.

## Checks

| Command | Result |
| --- | --- |
| `cargo test -p chaoscontrol-smr --test protocol-observation` | exit 0; 7 passed, 0 failed |
| `cargo test -p chaoscontrol-smr -p chaoscontrol-raft-guest` | exit 0; 168 passed, 0 failed |
| `cargo clippy -p chaoscontrol-smr -p chaoscontrol-evidence --all-targets -- --deny warnings` | exit 0 |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo test --no-fail-fast` (workspace) | exit 0; 3104 passed, 0 failed, 51 ignored |

The suite covers order-independent cohort and novelty identity, divergence detection when the runtime claims success, duplicate sequence, sequence gap, dropped events, missing final drain, removed record, mutated projection bytes, generation drift, record-byte overflow, duplicate participant, oracle identity reuse, unknown profile field, stale schema, zero bound, malformed chain projection, forged guest source, rollback, duplicate application, and stable-quorum stall classification.

## Remaining

Tasks left open are not evidenced here: committed Nickel profile fixtures, unknown-participant and unstable-novelty negatives, the SDK surface, VMM collection, a generic consumer oracle adapter, the first-party Raft and false self-oracle fixtures, explorer guidance, markers and snapshots, receipts, operator status, and closeout gates. Marker policy is `disabled`.

## Evidence boundary

Cohort completeness proves observation structure only. It does not prove protocol correctness, snapshot-backed replay, or production readiness. No guest campaign ran.
