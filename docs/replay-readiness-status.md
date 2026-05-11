# Replay Readiness Status

Generated from `dogfood-results/accepted-workload-proofs.json`. Do not hand-edit this file; run `cargo run -p chaoscontrol-evidence --bin generate-replay-readiness-report -- --write .`.

## Summary

ChaosControl currently supports bounded snapshot-backed replay proof claims for: `raft`, `redb`, `net`, `rust-workload`.

Current product target: Rust-only workload support on one machine with multiple local ChaosControl hypervisors. The remaining product gaps are local multi-hypervisor control-plane depth, Rust workload authoring/onboarding, bounded determinism/fault coverage, local triage, and local artifact hygiene. Hosted services, cross-machine fleet scheduling, and non-Rust SDKs are out of current product scope even though their claims remain forbidden overclaims.

This status is evidence-backed but narrow: it is not a mathematical determinism proof, not a universal hypervisor/device/timing proof, and not a full Antithesis-style product replacement claim.

## Supported bounded replay surfaces

| Workload | Status | Assertion ID | Accepted verdict | Replay parent depth | export/reproduce exit | Evidence |
| --- | --- | ---: | --- | ---: | --- | --- |
| `raft` | `supported-bounded` | `1806003755` | `snapshot_backed_reproduced` | `2` | `0` / `0` | `dogfood-results/raft-accepted-verdict-dogfood-20260509T030143Z/` |
| `redb` | `supported-bounded` | `2718281828` | `snapshot_backed_reproduced` | `1` | `0` / `0` | `dogfood-results/redb-accepted-verdict-dogfood-20260510T191449Z/` |
| `net` | `supported-bounded` | `3141592653` | `snapshot_backed_reproduced` | `1` | `0` / `0` | `dogfood-results/net-accepted-verdict-dogfood-20260509T015147Z/` |
| `rust-workload` | `supported-bounded` | `1414213562` | `snapshot_backed_reproduced` | `2` | `0` / `0` | `dogfood-results/rust-workload-accepted-verdict-dogfood-20260511T163054Z/` |

Supported here means the committed evidence contains an accepted summary, exported bug artifact, Rust-owned replay verdict, `replay_parent_depth > 0`, and either a present digest-matching `.snapshot.bin` artifact or a verified chunk manifest sidecar validated by the Rust `check-replay-proof-coverage` gate.

## Experimental or unproven surfaces

| Surface | Status | Why it is not promoted | Required promotion evidence |
| --- | --- | --- | --- |
| Rust workload authoring | `experimental-rust-only` | New Rust workloads need their own bounded probe, accepted verdict, manifest entry, and committed raw or chunked snapshot artifact before promotion. Non-Rust SDKs are not current product blockers. | Committed Rust workload recipe, accepted-verdict wrapper expectation, manifest entry, snapshot artifact, and replay/assertion readiness checks for that Rust workload. |
| Schedule-only replay | `gap-evidence-only` | Depth-zero replay results classify replay gaps; they do not prove snapshot-backed replay coverage. | A reproduced bug with `replay_parent_depth > 0`, valid snapshot ref/artifact or chunks, and `snapshot_backed_reproduced` verdict. |
| Arbitrary guest/device determinism | `bounded-matrix-rail` | Current evidence includes a bounded hide-TSC device/profile matrix rail (`nix run .#vm-determinism-matrix`) that emits a `matrix-receipt.json` from listed VM determinism observations. Matrix rows bind named single-machine multi-hypervisor product profiles, worker counts, workload identity, kernel/initrd fingerprints, device profile, clock profile, and controller configuration. This is matrix-scoped evidence only; unlisted guests, devices, clock profiles, and timing behaviors remain unproven, and the rail is not a universal hypervisor/device/timing determinism proof. | Committed device/profile matrix receipts for each promoted row, visible failing/unsupported rows with bounded mismatch details, negative drift evidence for unsupported profiles, and promotion-gate checks that reject converting the bounded matrix rail into an arbitrary or universal determinism claim. |
| Operator triage UX | `local-runbook` | Current evidence includes a committed local operator triage runbook generated from replay-readiness receipts and accepted proof artifacts. It records local decisions without raw-log scraping, but it is not a hosted service or fleet workflow. | A local triage runbook must stay generated from readiness receipts, open committed bug/replay artifacts, run reproduce/minimize, and record operator decisions without raw-log scraping. |
| Hosted/fleet triage UI | `non-goal-current-scope` | Hosted UI, SaaS service, and real cross-machine operator workflows are out of current product scope. Local operator triage remains bounded to generated runbooks and local decision receipts. | No current-scope promotion path; any future hosted/UI-backed fleet triage would need explicit scope reopening plus evidence that ingests readiness receipts, links bug/replay artifacts, persists shared operator decisions across real machine boundaries, and proves the workflow without raw-log scraping. |
| Local multi-hypervisor control plane | `active-local-gap` | Current evidence includes bounded local sequential scheduler execution, a bounded local multi-hypervisor campaign receipt, and a real KVM multi-hypervisor smoke rail. The current product gap is a stronger one-machine control plane with resource budgets, artifact roots, follow-up jobs, and durable state for multiple local hypervisor workers. | A committed single-machine multi-hypervisor control-plane receipt that binds worker budgets, artifact roots, queue state transitions, run receipts, bug follow-up jobs, and local artifact retention without raw-log scraping or hosted/cross-machine claims. |
| FoundationDB-style in-process deterministic simulator | `adapter-simulator-receipt` | Current evidence includes a Rust-owned in-process simulator adapter receipt emitted by `in-process-simulator-receipt`; it binds deterministic scheduler, virtual clock, RNG, simulated network/disk hooks, fault schedule, history, output digests, and sim-vm bridge metadata for workload/adapter/scenario comparison. This is adapter-simulator evidence only: not VM replay proof, not arbitrary binary support, and not full FoundationDB parity. | Committed simulator receipts for promoted workload adapters, negative nondeterminism fixtures, sim-vm bridge comparisons that preserve simulator-local vs vm-snapshot-replay evidence classes, readiness gates that reject VM-replay or full-FoundationDB overclaims, and separate VMM replay evidence before any replay-product claim. |
| Full Antithesis-style product replacement | `non-goal-current-scope` | Full Antithesis-style hosted product replacement is not the current product target; no hosted service, broad workload catalog, fleet-scale scheduler, UI, or formal determinism theorem is claimed by this evidence. | No current-scope promotion path; no existing bounded local/Rust rail may imply full Antithesis-style product parity. |

## Promotion rule

A new surface can move into `supported-bounded` only after it has committed evidence in the accepted workload manifest and all of these checks pass:

```bash
cargo run -p chaoscontrol-evidence --bin check-replay-proof-coverage -- .
cargo run -p chaoscontrol-evidence --bin generate-replay-readiness-report -- --check .
cargo run -p chaoscontrol-evidence --bin check-readiness-promotion-gate -- --root .
nix build .#checks.x86_64-linux.evidence-contracts --no-link -L
```
