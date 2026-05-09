# Assertion Readiness Status

Generated from `dogfood-results/accepted-workload-proofs.json` and each committed `assertions.json`. Do not hand-edit this file; run `python scripts/generate-assertion-readiness-report.py --write`.

## Summary

This report is an assertion-density and uncovered-catalog view over accepted replay evidence. It helps decide whether a workload is richly instrumented enough to be a credible Antithesis-alternative rail, but it is not replay proof by itself.

## Accepted proof assertion coverage

| Workload | Cataloged | Exercised | always / sometimes / reachability / unreachable | Uncategorized | Non-passing | Evidence |
| --- | ---: | ---: | --- | ---: | ---: | --- |
| `raft` | `43` | `42` | `11` / `15` / `17` / `0` | `43` | `2` | `dogfood-results/raft-accepted-verdict-dogfood-20260509T030143Z/assertions.json` |
| `redb` | `27` | `18` | `17` / `2` / `8` / `0` | `27` | `10` | `dogfood-results/redb-accepted-verdict-dogfood-20260509T025029Z/assertions.json` |
| `net` | `5` | `5` | `3` / `2` / `0` / `0` | `5` | `1` | `dogfood-results/net-accepted-verdict-dogfood-20260509T015147Z/assertions.json` |
| `rust-workload` | `6` | `6` | `3` / `2` / `1` / `0` | `6` | `1` | `dogfood-results/rust-workload-accepted-verdict-dogfood-20260509T031107Z/assertions.json` |

## Promotion guidance

Before promoting a workload beyond a bounded replay proof, review these gaps and either add meaningful assertion categories/coverage or explicitly document why the remaining gaps are acceptable for that workload:

- raft: 1 unhit assertion(s)
- raft: 43 uncategorized assertion(s)
- raft: 2 non-passing assertion(s)
- redb: 9 unhit assertion(s)
- redb: 27 uncategorized assertion(s)
- redb: 10 non-passing assertion(s)
- net: 0 unhit assertion(s)
- net: 5 uncategorized assertion(s)
- net: 1 non-passing assertion(s)
- rust-workload: 0 unhit assertion(s)
- rust-workload: 6 uncategorized assertion(s)
- rust-workload: 1 non-passing assertion(s)

## Anti-claim

A high exercised count only says the committed run observed cataloged SDK assertions. Product parity still requires workload setup ergonomics, replay evidence, minimization/reproduction UX, and operator triage surfaces outside this report.
