# Replay Proof Coverage

ChaosControl currently has accepted snapshot-backed replay proof coverage for the workloads listed in `dogfood-results/accepted-workload-proofs.json`.

| Workload | Assertion ID | Evidence | Verdict |
| --- | ---: | --- | --- |
| Raft | `1806003755` | `dogfood-results/raft-accepted-filtered-export-dogfood-20260507-014114/` | `snapshot_backed_reproduced` |
| redb | `2718281828` | `dogfood-results/redb-accepted-verdict-dogfood-20260507-020314/` | `snapshot_backed_reproduced` |
| net | `3141592653` | `dogfood-results/net-accepted-verdict-dogfood-20260507-025858/` | `snapshot_backed_reproduced` |

The manifest/check are intentionally conservative: every listed proof must have an accepted summary, exported bug artifact, replay verdict with `replay_class = snapshot_backed_reproduced`, `reproduced = true`, `command.exit_status = 0`, `replay_parent_depth > 0`, and either a present digest-matching `.snapshot.bin` artifact or a verified `.snapshot.bin.chunks.json` sidecar whose ordered chunks reconstruct to the referenced digest.

This is workload coverage evidence, not a mathematical or universal determinism proof. It only supports claims about the named bounded workload rails and their committed verdict/snapshot artifacts. Operator-facing supported vs experimental status is generated in `docs/replay-readiness-status.md`. New breadth claims should add a manifest entry plus committed evidence and pass:

```bash
python scripts/check-replay-proof-coverage.py
python scripts/generate-replay-readiness-report.py --check
```
