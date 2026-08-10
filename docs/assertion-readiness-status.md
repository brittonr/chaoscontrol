# Assertion Readiness Status

Generated from `dogfood-results/accepted-workload-proofs.json` and each committed `assertions.json`. Do not hand-edit this file; run `cargo run -p chaoscontrol-evidence --bin generate-assertion-readiness-report -- --write .`.

## Summary

This report is an assertion-density and uncovered-catalog view over historical replay evidence plus explicitly-labeled deterministic local assertion harnesses. It helps decide whether a workload is richly instrumented enough to be a credible Antithesis-alternative rail, but it is not replay proof by itself.

Legacy bare-array assertion artifacts are diagnostic-only. Only an accepted v2 summary with a complete admitted structured catalog can qualify for promotion.

## Assertion evidence status

| Workload | Identity status | Cataloged | Exercised | always / sometimes / reachability / unreachable | Uncategorized | Non-passing | Replay probe failures | Evidence |
| --- | --- | ---: | ---: | --- | ---: | ---: | ---: | --- |
| `raft` | `accepted-v2` | `44` | `13` | `12` / `15` / `17` / `0` | `0` | `35` | `1` | `dogfood-results/raft-fresh-v2-proof-20260809/assertions.json` |
| `redb` | `accepted-v2` | `27` | `13` | `17` / `2` / `8` / `0` | `0` | `14` | `1` | `dogfood-results/redb-fresh-v2-proof-20260809/assertions.json` |
| `net` | `accepted-v2` | `5` | `1` | `3` / `2` / `0` / `0` | `0` | `4` | `1` | `dogfood-results/net-fresh-v2-proof-20260809/assertions.json` |
| `rust-workload` | `accepted-v2` | `6` | `6` | `3` / `2` / `1` / `0` | `0` | `0` | `1` | `dogfood-results/rust-workload-fresh-v2-proof-20260809/assertions.json` |

## Promotion guidance

Before promotion, each workload must have accepted v2 assertion identity. Category or coverage rationale cannot waive this identity requirement.

- raft: 31 unhit assertion(s)
- raft: 0 uncategorized assertion(s)
- raft: 35 non-passing assertion(s)
- redb: 14 unhit assertion(s)
- redb: 0 uncategorized assertion(s)
- redb: 14 non-passing assertion(s)
- net: 4 unhit assertion(s)
- net: 0 uncategorized assertion(s)
- net: 4 non-passing assertion(s)
- rust-workload: 0 unhit assertion(s)
- rust-workload: 0 uncategorized assertion(s)
- rust-workload: 0 non-passing assertion(s)

## Replay proof signals

Replay-probe failures are controlled proof signals, not ordinary application failures. Fresh authority requires strict identity, receipt, snapshot, and replay validation; legacy signals remain diagnostic-only.

- net: `net snapshot replay probe trips only after restored parent context` (kind=always, category=replay-probe (inferred), verdict=failed, hit_count=102)
- raft: `snapshot replay probe trips only after restored parent context` (kind=always, category=replay-probe (inferred), verdict=failed, hit_count=8)
- redb: `redb snapshot replay probe trips only after restored parent context` (kind=always, category=replay-probe (inferred), verdict=failed, hit_count=1)
- rust-workload: `rust workload snapshot replay probe trips only after restored parent context` (kind=always, category=replay-probe (inferred), verdict=failed, hit_count=13)

## Gap details

These details are derived from committed historical `assertions.json` artifacts, deterministic report-local category inference, and optional local assertion harness fixtures. Inferred categories and local-harness coverage are marked. No fresh VM campaign is implied.

- net / non-passing: `client gets 3+ pongs` (kind=sometimes, category=operation, verdict=unexercised, hit_count=0)
- net / non-passing: `client receives correct pong` (kind=always, category=operation, verdict=unexercised, hit_count=0)
- net / non-passing: `server handles multiple pings` (kind=sometimes, category=operation, verdict=unexercised, hit_count=0)
- net / non-passing: `server responds to ping` (kind=always, category=operation, verdict=unexercised, hit_count=0)
- net / unhit: `client gets 3+ pongs` (kind=sometimes, category=operation, verdict=unexercised, hit_count=0)
- net / unhit: `client receives correct pong` (kind=always, category=operation, verdict=unexercised, hit_count=0)
- net / unhit: `server handles multiple pings` (kind=sometimes, category=operation, verdict=unexercised, hit_count=0)
- net / unhit: `server responds to ping` (kind=always, category=operation, verdict=unexercised, hit_count=0)
- raft / non-passing: `3+ values committed` (kind=sometimes, category=branch, verdict=failed, hit_count=8)
- raft / non-passing: `append accepted` (kind=sometimes, category=branch, verdict=unexercised, hit_count=0)
- raft / non-passing: `append rejected` (kind=sometimes, category=branch, verdict=unexercised, hit_count=0)
- raft / non-passing: `append_entries handler` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / non-passing: `append_entries_response handler` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / non-passing: `candidate stepped down on append_entries` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / non-passing: `candidate won election` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / non-passing: `commit_index within log bounds` (kind=always, category=invariant, verdict=unexercised, hit_count=0)
- raft / non-passing: `commit_index within log bounds` (kind=always, category=invariant, verdict=unexercised, hit_count=0)
- raft / non-passing: `election timeout fired` (kind=sometimes, category=branch, verdict=failed, hit_count=9)
- raft / non-passing: `follower started election` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / non-passing: `leader commit advanced after replication` (kind=sometimes, category=branch, verdict=unexercised, hit_count=0)
- raft / non-passing: `leader elected` (kind=sometimes, category=branch, verdict=failed, hit_count=8)
- raft / non-passing: `leader proposed value` (kind=sometimes, category=branch, verdict=unexercised, hit_count=0)
- raft / non-passing: `leader self match_index tracks log` (kind=always, category=invariant, verdict=unexercised, hit_count=0)
- raft / non-passing: `leader skipped proposal` (kind=sometimes, category=branch, verdict=unexercised, hit_count=0)
- raft / non-passing: `link partitioned` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / non-passing: `log conflict: truncated` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / non-passing: `log entries consistent` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / non-passing: `match_index within bounds` (kind=always, category=invariant, verdict=unexercised, hit_count=0)
- raft / non-passing: `message delivered` (kind=sometimes, category=branch, verdict=unexercised, hit_count=0)
- raft / non-passing: `message dropped` (kind=sometimes, category=branch, verdict=unexercised, hit_count=0)
- raft / non-passing: `message duplicated` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / non-passing: `message reordered` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / non-passing: `new entries appended` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / non-passing: `next_index stays positive` (kind=always, category=invariant, verdict=unexercised, hit_count=0)
- raft / non-passing: `node restarted` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / non-passing: `partition healed` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / non-passing: `request_vote handler` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / non-passing: `request_vote_response handler` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / non-passing: `stepped down to follower` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / non-passing: `value committed` (kind=sometimes, category=branch, verdict=failed, hit_count=8)
- raft / non-passing: `vote denied` (kind=sometimes, category=branch, verdict=unexercised, hit_count=0)
- raft / non-passing: `vote granted` (kind=sometimes, category=branch, verdict=unexercised, hit_count=0)
- raft / non-passing: `voted_for matches granted candidate` (kind=always, category=invariant, verdict=unexercised, hit_count=0)
- raft / unhit: `append accepted` (kind=sometimes, category=branch, verdict=unexercised, hit_count=0)
- raft / unhit: `append rejected` (kind=sometimes, category=branch, verdict=unexercised, hit_count=0)
- raft / unhit: `append_entries handler` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / unhit: `append_entries_response handler` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / unhit: `candidate stepped down on append_entries` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / unhit: `candidate won election` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / unhit: `commit_index within log bounds` (kind=always, category=invariant, verdict=unexercised, hit_count=0)
- raft / unhit: `commit_index within log bounds` (kind=always, category=invariant, verdict=unexercised, hit_count=0)
- raft / unhit: `follower started election` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / unhit: `leader commit advanced after replication` (kind=sometimes, category=branch, verdict=unexercised, hit_count=0)
- raft / unhit: `leader proposed value` (kind=sometimes, category=branch, verdict=unexercised, hit_count=0)
- raft / unhit: `leader self match_index tracks log` (kind=always, category=invariant, verdict=unexercised, hit_count=0)
- raft / unhit: `leader skipped proposal` (kind=sometimes, category=branch, verdict=unexercised, hit_count=0)
- raft / unhit: `link partitioned` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / unhit: `log conflict: truncated` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / unhit: `log entries consistent` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / unhit: `match_index within bounds` (kind=always, category=invariant, verdict=unexercised, hit_count=0)
- raft / unhit: `message delivered` (kind=sometimes, category=branch, verdict=unexercised, hit_count=0)
- raft / unhit: `message dropped` (kind=sometimes, category=branch, verdict=unexercised, hit_count=0)
- raft / unhit: `message duplicated` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / unhit: `message reordered` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / unhit: `new entries appended` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / unhit: `next_index stays positive` (kind=always, category=invariant, verdict=unexercised, hit_count=0)
- raft / unhit: `node restarted` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / unhit: `partition healed` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / unhit: `request_vote handler` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / unhit: `request_vote_response handler` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / unhit: `stepped down to follower` (kind=reachability, category=branch, verdict=unexercised, hit_count=0)
- raft / unhit: `vote denied` (kind=sometimes, category=branch, verdict=unexercised, hit_count=0)
- raft / unhit: `vote granted` (kind=sometimes, category=branch, verdict=unexercised, hit_count=0)
- raft / unhit: `voted_for matches granted candidate` (kind=always, category=invariant, verdict=unexercised, hit_count=0)
- redb / non-passing: `commit succeeds` (kind=sometimes, category=operation, verdict=unexercised, hit_count=0)
- redb / non-passing: `delete removes key` (kind=always, category=invariant, verdict=unexercised, hit_count=0)
- redb / non-passing: `op: compact` (kind=reachability, category=operation, verdict=unexercised, hit_count=0)
- redb / non-passing: `op: delete` (kind=reachability, category=operation, verdict=unexercised, hit_count=0)
- redb / non-passing: `op: insert` (kind=reachability, category=operation, verdict=unexercised, hit_count=0)
- redb / non-passing: `op: range scan` (kind=reachability, category=operation, verdict=unexercised, hit_count=0)
- redb / non-passing: `op: read` (kind=reachability, category=operation, verdict=unexercised, hit_count=0)
- redb / non-passing: `op: rollback` (kind=reachability, category=operation, verdict=unexercised, hit_count=0)
- redb / non-passing: `op: savepoint` (kind=reachability, category=operation, verdict=unexercised, hit_count=0)
- redb / non-passing: `range scan entry matches oracle` (kind=always, category=invariant, verdict=unexercised, hit_count=0)
- redb / non-passing: `range scan length matches oracle` (kind=always, category=invariant, verdict=unexercised, hit_count=0)
- redb / non-passing: `read matches oracle (none)` (kind=always, category=invariant, verdict=unexercised, hit_count=0)
- redb / non-passing: `read matches oracle` (kind=always, category=invariant, verdict=unexercised, hit_count=0)
- redb / non-passing: `table len matches oracle` (kind=always, category=invariant, verdict=unexercised, hit_count=0)
- redb / unhit: `commit succeeds` (kind=sometimes, category=operation, verdict=unexercised, hit_count=0)
- redb / unhit: `delete removes key` (kind=always, category=invariant, verdict=unexercised, hit_count=0)
- redb / unhit: `op: compact` (kind=reachability, category=operation, verdict=unexercised, hit_count=0)
- redb / unhit: `op: delete` (kind=reachability, category=operation, verdict=unexercised, hit_count=0)
- redb / unhit: `op: insert` (kind=reachability, category=operation, verdict=unexercised, hit_count=0)
- redb / unhit: `op: range scan` (kind=reachability, category=operation, verdict=unexercised, hit_count=0)
- redb / unhit: `op: read` (kind=reachability, category=operation, verdict=unexercised, hit_count=0)
- redb / unhit: `op: rollback` (kind=reachability, category=operation, verdict=unexercised, hit_count=0)
- redb / unhit: `op: savepoint` (kind=reachability, category=operation, verdict=unexercised, hit_count=0)
- redb / unhit: `range scan entry matches oracle` (kind=always, category=invariant, verdict=unexercised, hit_count=0)
- redb / unhit: `range scan length matches oracle` (kind=always, category=invariant, verdict=unexercised, hit_count=0)
- redb / unhit: `read matches oracle (none)` (kind=always, category=invariant, verdict=unexercised, hit_count=0)
- redb / unhit: `read matches oracle` (kind=always, category=invariant, verdict=unexercised, hit_count=0)
- redb / unhit: `table len matches oracle` (kind=always, category=invariant, verdict=unexercised, hit_count=0)

## Local deterministic assertion harness coverage

- raft: `commits advance when quorum healthy` covered by local deterministic harness `crates/chaoscontrol-raft-guest/src/lib.rs::raft_local_assertion_harness_covers_quorum_commit_progress` (historical verdict=unexercised, hit_count=0)
- redb: `committed data survives restart` covered by local deterministic harness `crates/chaoscontrol-redb-guest/src/lib.rs::redb_local_assertion_harness_covers_readiness_gap_conditions` (historical verdict=unexercised, hit_count=0)
- redb: `committed key missing after recovery` covered by local deterministic harness `crates/chaoscontrol-redb-guest/src/lib.rs::redb_local_assertion_harness_covers_readiness_gap_conditions` (historical verdict=unexercised, hit_count=0)
- redb: `data survives compaction` covered by local deterministic harness `crates/chaoscontrol-redb-guest/src/lib.rs::redb_local_assertion_harness_covers_readiness_gap_conditions` (historical verdict=unexercised, hit_count=0)
- redb: `database opens after repair` covered by local deterministic harness `crates/chaoscontrol-redb-guest/src/lib.rs::redb_local_assertion_harness_covers_readiness_gap_conditions` (historical verdict=unexercised, hit_count=0)
- redb: `database opens after repair` covered by local deterministic harness `crates/chaoscontrol-redb-guest/src/lib.rs::redb_local_assertion_harness_covers_readiness_gap_conditions` (historical verdict=unexercised, hit_count=0)
- redb: `range scan empty table matches oracle` covered by local deterministic harness `crates/chaoscontrol-redb-guest/src/lib.rs::redb_local_assertion_harness_covers_readiness_gap_conditions` (historical verdict=unexercised, hit_count=0)
- redb: `read matches oracle (no table)` covered by local deterministic harness `crates/chaoscontrol-redb-guest/src/lib.rs::redb_local_assertion_harness_covers_readiness_gap_conditions` (historical verdict=unexercised, hit_count=0)
- redb: `table len matches oracle (no table)` covered by local deterministic harness `crates/chaoscontrol-redb-guest/src/lib.rs::redb_local_assertion_harness_covers_readiness_gap_conditions` (historical verdict=unexercised, hit_count=0)
- redb: `uncommitted data not visible` covered by local deterministic harness `crates/chaoscontrol-redb-guest/src/lib.rs::redb_local_assertion_harness_covers_readiness_gap_conditions` (historical verdict=unexercised, hit_count=0)

## Operator interpretation

Zero ordinary assertion blockers applies only to accepted v2 assertion evidence after deterministic local harness coverage is applied. Diagnostic-only rows cannot promote. Any future accepted result is an instrumentation-readiness signal only. It does not establish hosted-product parity. Operator/product readiness still requires separate replay, minimization/reproduction, workload onboarding, and triage evidence.

## Anti-claim

A high exercised count only says the committed run observed cataloged SDK assertions or that a clearly-labeled local deterministic harness covered a previously unhit assertion condition. Local harness coverage is not snapshot replay evidence. Replay-probe failure visibility is proof-signal accounting, not an application invariant failure. Product parity still requires workload setup ergonomics, replay evidence, minimization/reproduction UX, and operator triage surfaces outside this report.
