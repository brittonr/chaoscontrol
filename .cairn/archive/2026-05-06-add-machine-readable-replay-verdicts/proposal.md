# Proposal: Machine-readable replay verdicts

## Why

ChaosControl now has a repeatable snapshot-backed replay smoke gate, but the final proof classification is still assembled from shell-script checks, reproduce log text, and dogfood receipt conventions. Operators and future dashboards need a first-class Rust-emitted verdict artifact that distinguishes snapshot-backed replay success from schedule-only gaps, missing artifacts, digest failures, and ordinary non-reproduction.

## What Changes

- Add a machine-readable replay verdict artifact emitted by replay/smoke/evidence paths.
- Classify replay attempts using explicit stable classes such as `snapshot_backed_reproduced`, `snapshot_backed_not_reproduced`, `schedule_only_replay_gap`, `missing_snapshot_ref`, `invalid_snapshot_digest`, and `no_bug_found`.
- Bind each verdict to bug path, assertion id, replay parent depth, snapshot reference validation, reproduce exit status, concise diagnostics, and artifact hashes.
- Extend dogfood/evidence validation so accepted snapshot-backed proof requires a replay verdict rather than log scraping.

## Scope

In scope:
- Rust-owned replay verdict data model and JSON serialization.
- CLI/smoke output that writes a concise verdict artifact.
- Contract/checker updates for verdict shape and accepted classifications.
- Documentation for interpreting replayability versus broader hypervisor determinism.

Out of scope:
- Proving global deterministic hypervisor behavior across all devices and workloads.
- Replacing raw debug logs; logs remain optional local diagnostics.
- Adding a dashboard UI beyond producing the artifact a dashboard can consume.

## Impact

- Affected capabilities: replay parent snapshots, dogfood evidence contracts, snapshot replay smoke gate.
- Affected files likely include `chaoscontrol-explore` replay/export/smoke plumbing, evidence contracts/checkers, README, and Nix smoke output.
- Verification: Rust serialization tests, positive/negative evidence fixtures, the existing evidence contract gates, and the KVM snapshot replay smoke check.
