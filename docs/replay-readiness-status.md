# Replay Readiness Status

Generated from `dogfood-results/accepted-workload-proofs.json`. Do not hand-edit this file; run `cargo run -p chaoscontrol-evidence --bin generate-replay-readiness-report -- --write .`.

## Summary

ChaosControl currently supports bounded snapshot-backed replay proof claims for: `raft`, `redb`, `net`, `rust-workload`.

This status is evidence-backed but narrow: it is not a mathematical determinism proof, not a universal hypervisor/device/timing proof, and not a full Antithesis-style product replacement claim.

## Supported bounded replay surfaces

| Workload | Status | Assertion ID | Accepted verdict | Replay parent depth | export/reproduce exit | Evidence |
| --- | --- | ---: | --- | ---: | --- | --- |
| `raft` | `supported-bounded` | `1806003755` | `snapshot_backed_reproduced` | `2` | `0` / `0` | `dogfood-results/raft-accepted-verdict-dogfood-20260509T030143Z/` |
| `redb` | `supported-bounded` | `2718281828` | `snapshot_backed_reproduced` | `1` | `0` / `0` | `dogfood-results/redb-accepted-verdict-dogfood-20260510T191449Z/` |
| `net` | `supported-bounded` | `3141592653` | `snapshot_backed_reproduced` | `1` | `0` / `0` | `dogfood-results/net-accepted-verdict-dogfood-20260509T015147Z/` |
| `rust-workload` | `supported-bounded` | `1414213562` | `snapshot_backed_reproduced` | `2` | `0` / `0` | `dogfood-results/rust-workload-accepted-verdict-dogfood-20260509T031107Z/` |

Supported here means the committed evidence contains an accepted summary, exported bug artifact, Rust-owned replay verdict, `replay_parent_depth > 0`, and either a present digest-matching `.snapshot.bin` artifact or a verified chunk manifest sidecar validated by the Rust `check-replay-proof-coverage` gate.

## Experimental or unproven surfaces

| Surface | Status | Why it is not promoted | Required promotion evidence |
| --- | --- | --- | --- |
| Fresh workload authoring | `experimental` | New workloads need their own bounded probe, accepted verdict, manifest entry, and committed raw or chunked snapshot artifact before promotion. | Committed workload recipe, accepted-verdict wrapper expectation, manifest entry, snapshot artifact, and replay/assertion readiness checks for that workload. |
| Schedule-only replay | `gap-evidence-only` | Depth-zero replay results classify replay gaps; they do not prove snapshot-backed replay coverage. | A reproduced bug with `replay_parent_depth > 0`, valid snapshot ref/artifact or chunks, and `snapshot_backed_reproduced` verdict. |
| Arbitrary guest/device determinism | `unproven` | Current evidence covers named bounded workload rails only. The bounded hide-TSC VM drift gate receipt at `dogfood-results/vm-determinism-hide-tsc-broader-2026-05-10/receipt.json` passes across selected single-VM and controller configurations, but it is a profile-specific drift check rather than a universal hypervisor/device/timing determinism proof. | Broader device/profile matrix receipts plus negative drift evidence; a bounded drift gate must not be promoted into a universal theorem. |
| Operator triage UX | `local-runbook` | Current evidence includes a committed local operator triage runbook generated from replay-readiness receipts and accepted proof artifacts. It records local decisions without raw-log scraping, but it is not a hosted service or fleet workflow. | A local triage runbook must stay generated from readiness receipts, open committed bug/replay artifacts, run reproduce/minimize, and record operator decisions without raw-log scraping. |
| Hosted/fleet triage UI | `local-decision-receipts` | Current evidence includes a static multi-receipt fleet triage index plus a bounded local operator decision receipt format, but there is still no hosted UI, shared decision store, or cross-machine operator workflow evidence. | Hosted or UI-backed fleet triage evidence that ingests readiness receipts from multiple runs, links bug/replay artifacts, runs or records reproduce/minimize outcomes, persists shared operator decisions, and proves the workflow without raw-log scraping. |
| Replay scheduler orchestration | `bounded-fleet-scheduler-receipt` | Current evidence includes bounded local sequential scheduler execution plus a durable queue/lease/worker/run receipt model for hosted/fleet scheduler review, but not a running hosted service or automatic campaign service. | A running hosted scheduler integration that executes multiple replay-readiness runs across machines, persists queue state, links each run to receipt artifacts and local decisions, enforces bounded concurrency/failure behavior, and proves the workflow without raw-log scraping. |
| Full Antithesis-style product replacement | `not-supported` | No hosted service, broad workload catalog, fleet-scale scheduler, UI, or formal determinism theorem is claimed by this evidence. | Separate hosted-service, scheduler, workload catalog, UI, fleet, and formal determinism evidence; no existing bounded rail may imply this status. |

## Promotion rule

A new surface can move into `supported-bounded` only after it has committed evidence in the accepted workload manifest and all of these checks pass:

```bash
cargo run -p chaoscontrol-evidence --bin check-replay-proof-coverage -- .
cargo run -p chaoscontrol-evidence --bin generate-replay-readiness-report -- --check .
nix build .#checks.x86_64-linux.evidence-contracts --no-link -L
```
