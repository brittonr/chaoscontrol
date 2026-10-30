# Validation evidence

Date: 2026-10-30. Source: worktree `cairn/molten-contracts-20261029` (published tree without the SpaceWasm rail) on base `1caeda2334f1d93e53ce22afbfa012b05458f606`, run in `nix develop` with `CARGO_TARGET_DIR`, `TMPDIR`, and `XDG_CACHE_HOME` under `/home/brittonr/.cargo-target/`.

## Baseline

The old SMR unit expectation (`history_accepts_lag_and_rejects_divergence_rollback_and_gaps`) required an identical older-index observation to pass as a harmless duplicate. With the rollback check in place, `cargo test -p chaoscontrol-smr -p chaoscontrol-raft-guest` exited 101 at that assertion (39 passed, 1 failed; retained in `/home/brittonr/.cargo-target/verify-chaos-abi-20261030/smr-raft.*`). The expectation was replaced by the current-index positive and old-index rollback negative cases below.

## Checks

| Command | Result |
| --- | --- |
| `cargo test -p chaoscontrol-smr -p chaoscontrol-raft-guest` | exit 0; 168 passed, 0 failed |
| `cargo test -p chaoscontrol-smr --test protocol-observation` | exit 0; 7 passed, 0 failed (includes `rollback_and_duplicate_application_remain_chain_failures`) |
| `cargo clippy -p chaoscontrol-smr -p chaoscontrol-evidence --all-targets -- --deny warnings` | exit 0 |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo test --no-fail-fast` (workspace) | exit 0; 3104 passed, 0 failed, 51 ignored |

The unit regression asserts both directions: an identical repeat of the current highest index passes with one exact duplicate, and an identical repeat of index 1 after index 2 fails with a `Rollback` violation and one exact duplicate.

## Evidence boundary

This shows history validation behavior for the tested inputs. It does not prove consensus correctness, liveness, or snapshot-backed replay.
