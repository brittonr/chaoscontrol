# Replay Readiness Status

Generated from `dogfood-results/accepted-workload-proofs.json`. Do not hand-edit this file; run `python scripts/generate-replay-readiness-report.py --write`.

## Summary

ChaosControl currently supports bounded snapshot-backed replay proof claims for: `raft`, `redb`, `net`.

This status is evidence-backed but narrow: it is not a mathematical determinism proof, not a universal hypervisor/device/timing proof, and not a full Antithesis-style product replacement claim.

## Supported bounded replay surfaces

| Workload | Status | Assertion ID | Accepted verdict | Replay parent depth | export/reproduce exit | Evidence |
| --- | --- | ---: | --- | ---: | --- | --- |
| `raft` | `supported-bounded` | `1806003755` | `snapshot_backed_reproduced` | `2` | `0` / `0` | `dogfood-results/raft-accepted-filtered-export-dogfood-20260507-014114/` |
| `redb` | `supported-bounded` | `2718281828` | `snapshot_backed_reproduced` | `1` | `0` / `0` | `dogfood-results/redb-accepted-verdict-dogfood-20260507-020314/` |
| `net` | `supported-bounded` | `3141592653` | `snapshot_backed_reproduced` | `1` | `0` / `0` | `dogfood-results/net-accepted-verdict-dogfood-20260507-025858/` |

Supported here means the committed evidence contains an accepted summary, exported bug artifact, Rust-owned replay verdict, `replay_parent_depth > 0`, and either a present digest-matching `.snapshot.bin` artifact or a verified chunk manifest sidecar validated by `scripts/check-replay-proof-coverage.py`.

## Experimental or unproven surfaces

| Surface | Status | Why it is not promoted |
| --- | --- | --- |
| Fresh workload authoring | `experimental` | New workloads need their own bounded probe, accepted verdict, manifest entry, and committed raw or chunked snapshot artifact before promotion. |
| Schedule-only replay | `gap-evidence-only` | Depth-zero replay results classify replay gaps; they do not prove snapshot-backed replay coverage. |
| Arbitrary guest/device determinism | `unproven` | Current evidence covers named bounded workload rails only, not universal hypervisor/device/timing behavior. |
| Full Antithesis-style product replacement | `not-supported` | No hosted service, broad workload catalog, fleet-scale scheduler, UI, or formal determinism theorem is claimed by this evidence. |

## Promotion rule

A new surface can move into `supported-bounded` only after it has committed evidence in the accepted workload manifest and all of these checks pass:

```bash
python scripts/check-replay-proof-coverage.py
python scripts/generate-replay-readiness-report.py --check
nix build .#checks.x86_64-linux.evidence-contracts --no-link -L
```
