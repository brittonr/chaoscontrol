# Replay Readiness Status

Generated from `dogfood-results/accepted-workload-proofs.json`. Do not hand-edit this file; run `cargo run -p chaoscontrol-evidence --bin generate-replay-readiness-report -- --write .`.

## Summary

ChaosControl currently supports bounded snapshot-backed replay proof claims for: `raft`, `redb`, `net`, `rust-workload`.

Current product target: Rust-only workload support on one machine with multiple local ChaosControl hypervisors. The supported baseline covers the admitted Rust cohort and durable one-machine multi-hypervisor orchestration. Remaining gaps include broader workload admission, bounded determinism and fault coverage, local triage depth, and local artifact hygiene. Hosted services, cross-machine fleet scheduling, and non-Rust SDKs are out of current product scope.

This status is evidence-backed but narrow: it is not a mathematical determinism proof, not a universal hypervisor/device/timing proof, and not a full Antithesis-style product replacement claim.

## Bounded replay evidence promotion status

| Workload | Status | Assertion ID | Historical verdict | Replay parent depth | export/reproduce exit | Evidence |
| --- | --- | ---: | --- | ---: | --- | --- |
| `raft` | `supported-bounded` | `3463273124` | `snapshot_backed_reproduced` | `1` | `0` / `0` | `dogfood-results/raft-fresh-v2-proof-20260809/` |
| `redb` | `supported-bounded` | `4149728441` | `snapshot_backed_reproduced` | `1` | `0` / `0` | `dogfood-results/redb-fresh-v2-proof-20260809/` |
| `net` | `supported-bounded` | `2074476939` | `snapshot_backed_reproduced` | `1` | `0` / `0` | `dogfood-results/net-fresh-v2-proof-20260809/` |
| `rust-workload` | `supported-bounded` | `3143219316` | `snapshot_backed_reproduced` | `1` | `0` / `0` | `dogfood-results/rust-workload-fresh-v2-proof-20260809/` |

`blocked-assertion-identity` means that the retained replay files predate admitted v2 structured assertion identity. Numeric alias agreement cannot promote them. Fresh KVM evidence must bind the selected alias and complete catalog through an accepted v2 summary.

## Experimental or unproven surfaces

| Surface | Status | Why it is not promoted | Required promotion evidence |
| --- | --- | --- | --- |
| Rust workload authoring | `supported-bounded-rust-cohort` | The admitted downstream-shaped Rust cohort has a fresh strict-identity KVM proof and a bounded onboarding recipe. New or changed workload code needs its own proof. | Keep the admitted cohort receipt, accepted verdict, manifest entry, snapshot artifact, and replay/assertion readiness checks green. |
| Schedule-only replay | `gap-evidence-only` | Depth-zero replay results classify replay gaps; they do not prove snapshot-backed replay coverage. | A reproduced bug with `replay_parent_depth > 0`, valid snapshot ref/artifact or chunks, and `snapshot_backed_reproduced` verdict. |
| Arbitrary guest/device determinism | `bounded-matrix-rail` | Current evidence includes a bounded hide-TSC device/profile matrix rail (`nix run .#vm-determinism-matrix`) that emits a `matrix-receipt.json` from listed VM determinism observations. Matrix rows bind named single-machine multi-hypervisor product profiles, worker counts, workload identity, kernel/initrd fingerprints, device profile, clock profile, and controller configuration. This is matrix-scoped evidence only; unlisted guests, devices, clock profiles, and timing behaviors remain unproven, and the rail is not a universal hypervisor/device/timing determinism proof. | Committed device/profile matrix receipts for each promoted row, visible failing/unsupported rows with bounded mismatch details, negative drift evidence for unsupported profiles, and promotion-gate checks that reject converting the bounded matrix rail into an arbitrary or universal determinism claim. |
| Operator triage UX | `local-runbook` | Current evidence includes a committed local operator triage runbook generated from replay-readiness receipts and historical diagnostic artifacts. Its blocked sections do not run reproduction or minimization for ID-only bugs. It is not a hosted service or fleet workflow. | A promotable local triage path requires fresh admitted v2 KVM evidence, exact bug/report identity binding, replay and minimization commands, and operator decisions without raw-log scraping. |
| Hosted/fleet triage UI | `non-goal-current-scope` | Hosted UI, SaaS service, and real cross-machine operator workflows are out of current product scope. Local operator triage remains bounded to generated runbooks and local decision receipts. | No current-scope promotion path; any future hosted/UI-backed fleet triage would need explicit scope reopening plus evidence that ingests readiness receipts, links bug/replay artifacts, persists shared operator decisions across real machine boundaries, and proves the workflow without raw-log scraping. |
| Local multi-hypervisor control plane | `supported-bounded-local` | Current evidence includes bounded local sequential scheduler execution, a durable local multi-hypervisor campaign receipt, a real KVM multi-hypervisor smoke rail, worker resource budgets, artifact roots/indexes, queue-state transitions, run receipts, bug follow-up jobs, and local artifact retention. This is a supported one-machine local control-plane workflow only; it is not a hosted service, shared remote queue, cross-machine scheduler, universal fleet-scale throughput claim, or full Antithesis-style product replacement. | Keep the committed single-machine multi-hypervisor control-plane receipt, KVM smoke rail, worker budgets, artifact roots/indexes, queue-state transitions, run receipts, bug follow-up jobs, local artifact retention, and anti-overclaim gates green without raw-log scraping or hosted/cross-machine claims. |
| Adapter-based distributed protocol simulation | `adapter-protocol-simulation` | Current evidence includes adapter-based protocol-simulation receipts and one bounded partition-failure replay fixture. Receipts bind the seed, schedule, config, artifacts, history, and output. This is adapter-based protocol-simulation evidence only. It is separate from VM snapshot replay proof and in-process simulator evidence. It does not prove VM replay, arbitrary protocol correctness, or Celld-equivalent behavior. | Require committed receipts for supported adapters, negative nondeterminism and fault fixtures, and stable mismatch checks. Keep separate VMM and in-process evidence before broader promotion. |
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
