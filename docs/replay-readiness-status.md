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
| Arbitrary guest/device determinism | `bounded-matrix-rail` | Current evidence includes a bounded hide-TSC device/profile matrix rail (`nix run .#vm-determinism-matrix`) that emits a `matrix-receipt.json` from listed VM determinism observations. This is matrix-scoped evidence only; unlisted guests, devices, clock profiles, and timing behaviors remain unproven, and the rail is not a universal hypervisor/device/timing determinism proof. | Committed device/profile matrix receipts for each promoted row, negative drift evidence for unsupported profiles, and promotion-gate checks that reject converting the bounded matrix rail into an arbitrary or universal determinism claim. |
| Operator triage UX | `local-runbook` | Current evidence includes a committed local operator triage runbook generated from replay-readiness receipts and accepted proof artifacts. It records local decisions without raw-log scraping, but it is not a hosted service or fleet workflow. | A local triage runbook must stay generated from readiness receipts, open committed bug/replay artifacts, run reproduce/minimize, and record operator decisions without raw-log scraping. |
| Hosted/fleet triage UI | `bounded-shared-state-harness` | Current evidence includes a static multi-receipt fleet triage index, bounded local operator decision receipts, and a loopback hosted/shared-state harness that persists shared decision records with writer identities. There is still no hosted UI, SaaS service, or real cross-machine operator workflow evidence. | Hosted or UI-backed fleet triage evidence that ingests readiness receipts from multiple real runs, links bug/replay artifacts, runs or records reproduce/minimize outcomes, persists shared operator decisions across machine boundaries, and proves the workflow without raw-log scraping. |
| Replay scheduler orchestration | `bounded-shared-state-harness` | Current evidence includes bounded local sequential scheduler execution, a restart-persistent local hosted/fleet worker loop, a bounded local multi-hypervisor campaign receipt, a real KVM multi-hypervisor smoke rail, and a loopback hosted/shared-state harness that exercises shared queue leasing plus shared decision writes through the adapter boundary. It is still not a SaaS service, real cross-machine scheduler, universal fleet-scale scheduler, or Antithesis parity claim. | A multi-machine hosted scheduler integration that shares queue state across machines, links each run to receipt artifacts and shared decisions, enforces bounded concurrency/failure behavior across workers, and proves the workflow without raw-log scraping. |
| Full Antithesis-style product replacement | `not-supported` | No hosted service, broad workload catalog, fleet-scale scheduler, UI, or formal determinism theorem is claimed by this evidence. | Separate hosted-service, scheduler, workload catalog, UI, fleet, and formal determinism evidence; no existing bounded rail may imply this status. |

## Promotion rule

A new surface can move into `supported-bounded` only after it has committed evidence in the accepted workload manifest and all of these checks pass:

```bash
cargo run -p chaoscontrol-evidence --bin check-replay-proof-coverage -- .
cargo run -p chaoscontrol-evidence --bin generate-replay-readiness-report -- --check .
cargo run -p chaoscontrol-evidence --bin check-readiness-promotion-gate -- --root .
nix build .#checks.x86_64-linux.evidence-contracts --no-link -L
```
