# Assertion Readiness Status

Generated from `dogfood-results/accepted-workload-proofs.json` and each committed `assertions.json`. Do not hand-edit this file; run `cargo run -p chaoscontrol-evidence --bin generate-assertion-readiness-report -- --write .`.

## Summary

This report is an assertion-density and uncovered-catalog view over accepted replay evidence plus explicitly-labeled deterministic local assertion harnesses. It helps decide whether a workload is richly instrumented enough to be a credible Antithesis-alternative rail, but it is not replay proof by itself.

## Accepted proof assertion coverage

| Workload | Cataloged | Exercised | always / sometimes / reachability / unreachable | Uncategorized | Non-passing | Replay probe failures | Evidence |
| --- | ---: | ---: | --- | ---: | ---: | ---: | --- |
| `raft` | `43` | `43` | `11` / `15` / `17` / `0` | `0` | `0` | `1` | `dogfood-results/raft-accepted-verdict-dogfood-20260509T030143Z/assertions.json` |
| `redb` | `27` | `27` | `17` / `2` / `8` / `0` | `0` | `0` | `1` | `dogfood-results/redb-accepted-verdict-dogfood-20260510T191449Z/assertions.json` |
| `net` | `5` | `5` | `3` / `2` / `0` / `0` | `0` | `0` | `1` | `dogfood-results/net-accepted-verdict-dogfood-20260509T015147Z/assertions.json` |
| `rust-workload` | `6` | `6` | `3` / `2` / `1` / `0` | `0` | `0` | `1` | `dogfood-results/rust-workload-accepted-verdict-dogfood-20260509T031107Z/assertions.json` |

## Promotion guidance

Before promoting a workload beyond a bounded replay proof, review these gaps and either add meaningful assertion categories/coverage or explicitly document why the remaining gaps are acceptable for that workload:

- raft: 0 unhit assertion(s)
- raft: 0 uncategorized assertion(s)
- raft: 0 non-passing assertion(s)
- redb: 0 unhit assertion(s)
- redb: 0 uncategorized assertion(s)
- redb: 0 non-passing assertion(s)
- net: 0 unhit assertion(s)
- net: 0 uncategorized assertion(s)
- net: 0 non-passing assertion(s)
- rust-workload: 0 unhit assertion(s)
- rust-workload: 0 uncategorized assertion(s)
- rust-workload: 0 non-passing assertion(s)

## Replay proof signals

Replay-probe failures are intentional snapshot-replay proof signals. They remain checked evidence, but they are not ordinary instrumentation-readiness promotion blockers.

- net: `net snapshot replay probe trips only after restored parent context` (kind=always, category=replay-probe (inferred), verdict=failed, hit_count=9)
- raft: `snapshot replay probe trips only after restored parent context` (kind=always, category=replay-probe (inferred), verdict=failed, hit_count=2975)
- redb: `redb snapshot replay probe trips only after restored parent context` (kind=always, category=replay-probe (inferred), verdict=failed, hit_count=200)
- rust-workload: `rust workload snapshot replay probe trips only after restored parent context` (kind=always, category=replay-probe (inferred), verdict=failed, hit_count=10619)

## Gap details

These details are derived from committed accepted-proof `assertions.json` artifacts, deterministic report-local category inference, and optional local assertion harness fixtures; inferred categories and local-harness coverage are marked, and no fresh VM campaign is required.

- No unhit or non-passing assertion details in accepted proof artifacts.

## Local deterministic assertion harness coverage

- raft: `commits advance when quorum healthy` covered by local deterministic harness `crates/chaoscontrol-raft-guest/src/lib.rs::raft_local_assertion_harness_covers_quorum_commit_progress` (accepted-proof verdict=unexercised, hit_count=0)
- redb: `committed data survives restart` covered by local deterministic harness `crates/chaoscontrol-redb-guest/src/lib.rs::redb_local_assertion_harness_covers_readiness_gap_conditions` (accepted-proof verdict=unexercised, hit_count=0)
- redb: `committed key missing after recovery` covered by local deterministic harness `crates/chaoscontrol-redb-guest/src/lib.rs::redb_local_assertion_harness_covers_readiness_gap_conditions` (accepted-proof verdict=unexercised, hit_count=0)
- redb: `data survives compaction` covered by local deterministic harness `crates/chaoscontrol-redb-guest/src/lib.rs::redb_local_assertion_harness_covers_readiness_gap_conditions` (accepted-proof verdict=unexercised, hit_count=0)
- redb: `database opens after repair` covered by local deterministic harness `crates/chaoscontrol-redb-guest/src/lib.rs::redb_local_assertion_harness_covers_readiness_gap_conditions` (accepted-proof verdict=unexercised, hit_count=0)
- redb: `database opens after repair` covered by local deterministic harness `crates/chaoscontrol-redb-guest/src/lib.rs::redb_local_assertion_harness_covers_readiness_gap_conditions` (accepted-proof verdict=unexercised, hit_count=0)
- redb: `range scan empty table matches oracle` covered by local deterministic harness `crates/chaoscontrol-redb-guest/src/lib.rs::redb_local_assertion_harness_covers_readiness_gap_conditions` (accepted-proof verdict=unexercised, hit_count=0)
- redb: `read matches oracle (no table)` covered by local deterministic harness `crates/chaoscontrol-redb-guest/src/lib.rs::redb_local_assertion_harness_covers_readiness_gap_conditions` (accepted-proof verdict=unexercised, hit_count=0)
- redb: `table len matches oracle (no table)` covered by local deterministic harness `crates/chaoscontrol-redb-guest/src/lib.rs::redb_local_assertion_harness_covers_readiness_gap_conditions` (accepted-proof verdict=unexercised, hit_count=0)
- redb: `uncommitted data not visible` covered by local deterministic harness `crates/chaoscontrol-redb-guest/src/lib.rs::redb_local_assertion_harness_covers_readiness_gap_conditions` (accepted-proof verdict=unexercised, hit_count=0)

## Operator interpretation

Zero ordinary assertion blockers means only that accepted workload artifacts have no unhit, uncategorized, or non-replay-probe failing assertions after deterministic local harness coverage is applied. Read it as an instrumentation-readiness signal only: it does not establish hosted-product parity or promote a workload, device profile, or ChaosControl as a hosted product by itself. Operator/product readiness still requires separate replay, minimization/reproduction, workload onboarding, and triage evidence.

## Anti-claim

A high exercised count only says the committed run observed cataloged SDK assertions or that a clearly-labeled local deterministic harness covered a previously unhit assertion condition. Local harness coverage is not snapshot replay evidence. Replay-probe failure visibility is proof-signal accounting, not an application invariant failure. Product parity still requires workload setup ergonomics, replay evidence, minimization/reproduction UX, and operator triage surfaces outside this report.
