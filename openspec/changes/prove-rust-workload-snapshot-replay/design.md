## Context

`.#explore-rust-workload` now completes with a KCOV kernel and produces bounded VM campaign output. Existing accepted replay proof rails cover Raft, redb, and net via `scripts/accepted-snapshot-verdict-dogfood.py`, filtered `export-bugs`, persisted parent snapshot artifacts, and machine-readable replay verdict JSON.

## Goals / Non-Goals

**Goals:**

- Reuse the existing accepted snapshot verdict runner for the downstream-shaped Rust workload.
- Keep the probe opt-in via kernel cmdline so normal harness/local-report behavior stays clean.
- Preserve the evidence boundary: accepted proof requires snapshot ref validation and `snapshot_backed_reproduced`, not just a failing schedule.

**Non-Goals:**

- Change the SDK assertion API.
- Add non-Rust workload support.
- Commit raw run/reproduce logs or full checkpoints as primary evidence.

## Decisions

### 1. Opt-in probe in the sample Rust workload

**Choice:** Add `rust_workload_bug=snapshot_replay_probe` plus `rust_workload_snapshot_probe_fail_after=N` parsing to the downstream-shaped guest and guard a stable explicit assertion ID behind it.

**Rationale:** This mirrors the Raft/redb/net proof workloads while keeping ordinary sample behavior non-failing.

**Alternative:** Treat the prior VM campaign as proof. Rejected because it lacks replay parent depth and accepted replay verdict semantics.

### 2. Reuse the accepted verdict dogfood wrapper

**Choice:** Parameterize `scripts/accepted-snapshot-verdict-dogfood.py` with the Rust workload initrd, KCOV kernel, assertion ID, one VM, and a Rust cmdline template.

**Rationale:** The wrapper already enforces filtered export, snapshot artifact validation, and accepted replay verdict classification.

## Validation Plan

- Run targeted Rust guest/unit checks for cmdline parsing and probe gating.
- Build `.#kcov-vmlinux`, `.#initrd-rust-workload`, and `chaoscontrol-explore`.
- Run the accepted snapshot verdict wrapper for workload `rust-workload`.
- Curate concise evidence, update `dogfood-results/accepted-workload-proofs.json`, `docs/replay-proof-coverage.md`, and `docs/replay-readiness-status.md`.
- Run `python scripts/check-replay-proof-coverage.py`, `python scripts/generate-replay-readiness-report.py --check`, focused Rust tests, and `git diff --check`.
