# Assertion Readiness Status

Generated from `dogfood-results/accepted-workload-proofs.json` and each committed `assertions.json`. Do not hand-edit this file; run `cargo run -p chaoscontrol-evidence --bin generate-assertion-readiness-report -- --write .`.

## Summary

This report is an assertion-density and uncovered-catalog view over accepted replay evidence. It helps decide whether a workload is richly instrumented enough to be a credible Antithesis-alternative rail, but it is not replay proof by itself.

## Accepted proof assertion coverage

| Workload | Cataloged | Exercised | always / sometimes / reachability / unreachable | Uncategorized | Non-passing | Evidence |
| --- | ---: | ---: | --- | ---: | ---: | --- |
| `raft` | `43` | `42` | `11` / `15` / `17` / `0` | `0` | `2` | `dogfood-results/raft-accepted-verdict-dogfood-20260509T030143Z/assertions.json` |
| `redb` | `27` | `18` | `17` / `2` / `8` / `0` | `0` | `10` | `dogfood-results/redb-accepted-verdict-dogfood-20260509T025029Z/assertions.json` |
| `net` | `5` | `5` | `3` / `2` / `0` / `0` | `0` | `1` | `dogfood-results/net-accepted-verdict-dogfood-20260509T015147Z/assertions.json` |
| `rust-workload` | `6` | `6` | `3` / `2` / `1` / `0` | `0` | `1` | `dogfood-results/rust-workload-accepted-verdict-dogfood-20260509T031107Z/assertions.json` |

## Promotion guidance

Before promoting a workload beyond a bounded replay proof, review these gaps and either add meaningful assertion categories/coverage or explicitly document why the remaining gaps are acceptable for that workload:

- raft: 1 unhit assertion(s)
- raft: 0 uncategorized assertion(s)
- raft: 2 non-passing assertion(s)
- redb: 9 unhit assertion(s)
- redb: 0 uncategorized assertion(s)
- redb: 10 non-passing assertion(s)
- net: 0 unhit assertion(s)
- net: 0 uncategorized assertion(s)
- net: 1 non-passing assertion(s)
- rust-workload: 0 unhit assertion(s)
- rust-workload: 0 uncategorized assertion(s)
- rust-workload: 1 non-passing assertion(s)

## Gap details

These details are derived from committed accepted-proof `assertions.json` artifacts and deterministic report-local category inference; inferred categories are marked and no fresh VM campaign is required.

- net / non-passing: `net snapshot replay probe trips only after restored parent context` (kind=always, category=replay-probe (inferred), verdict=failed, hit_count=9)
- raft / non-passing: `commits advance when quorum healthy` (kind=sometimes, category=election (inferred), verdict=unexercised, hit_count=0)
- raft / non-passing: `snapshot replay probe trips only after restored parent context` (kind=always, category=replay-probe (inferred), verdict=failed, hit_count=2975)
- raft / unhit: `commits advance when quorum healthy` (kind=sometimes, category=election (inferred), verdict=unexercised, hit_count=0)
- redb / non-passing: `committed data survives restart` (kind=always, category=invariant (inferred), verdict=unexercised, hit_count=0)
- redb / non-passing: `committed key missing after recovery` (kind=always, category=invariant (inferred), verdict=unexercised, hit_count=0)
- redb / non-passing: `data survives compaction` (kind=always, category=invariant (inferred), verdict=unexercised, hit_count=0)
- redb / non-passing: `database opens after repair` (kind=always, category=invariant (inferred), verdict=unexercised, hit_count=0)
- redb / non-passing: `database opens after repair` (kind=always, category=invariant (inferred), verdict=unexercised, hit_count=0)
- redb / non-passing: `range scan empty table matches oracle` (kind=always, category=invariant (inferred), verdict=unexercised, hit_count=0)
- redb / non-passing: `read matches oracle (no table)` (kind=always, category=invariant (inferred), verdict=unexercised, hit_count=0)
- redb / non-passing: `redb snapshot replay probe trips only after restored parent context` (kind=always, category=replay-probe (inferred), verdict=failed, hit_count=195)
- redb / non-passing: `table len matches oracle (no table)` (kind=always, category=invariant (inferred), verdict=unexercised, hit_count=0)
- redb / non-passing: `uncommitted data not visible` (kind=always, category=invariant (inferred), verdict=unexercised, hit_count=0)
- redb / unhit: `committed data survives restart` (kind=always, category=invariant (inferred), verdict=unexercised, hit_count=0)
- redb / unhit: `committed key missing after recovery` (kind=always, category=invariant (inferred), verdict=unexercised, hit_count=0)
- redb / unhit: `data survives compaction` (kind=always, category=invariant (inferred), verdict=unexercised, hit_count=0)
- redb / unhit: `database opens after repair` (kind=always, category=invariant (inferred), verdict=unexercised, hit_count=0)
- redb / unhit: `database opens after repair` (kind=always, category=invariant (inferred), verdict=unexercised, hit_count=0)
- redb / unhit: `range scan empty table matches oracle` (kind=always, category=invariant (inferred), verdict=unexercised, hit_count=0)
- redb / unhit: `read matches oracle (no table)` (kind=always, category=invariant (inferred), verdict=unexercised, hit_count=0)
- redb / unhit: `table len matches oracle (no table)` (kind=always, category=invariant (inferred), verdict=unexercised, hit_count=0)
- redb / unhit: `uncommitted data not visible` (kind=always, category=invariant (inferred), verdict=unexercised, hit_count=0)
- rust-workload / non-passing: `rust workload snapshot replay probe trips only after restored parent context` (kind=always, category=replay-probe (inferred), verdict=failed, hit_count=10619)

## Anti-claim

A high exercised count only says the committed run observed cataloged SDK assertions. Product parity still requires workload setup ergonomics, replay evidence, minimization/reproduction UX, and operator triage surfaces outside this report.
