# Replay Proof Coverage

ChaosControl currently has accepted snapshot-backed replay proof coverage for the workloads listed in `dogfood-results/accepted-workload-proofs.json`.

| Workload | Assertion ID | Evidence | Verdict |
| --- | ---: | --- | --- |
| Raft | `1806003755` | `dogfood-results/raft-accepted-verdict-dogfood-20260509T030143Z/` | `snapshot_backed_reproduced` |
| redb | `2718281828` | `dogfood-results/redb-accepted-verdict-dogfood-20260510T191449Z/` | `snapshot_backed_reproduced` |
| net | `3141592653` | `dogfood-results/net-accepted-verdict-dogfood-20260509T015147Z/` | `snapshot_backed_reproduced` |
| rust-workload | `1414213562` | `dogfood-results/rust-workload-accepted-verdict-dogfood-20260511T163054Z/` | `snapshot_backed_reproduced` |

The manifest/check are intentionally conservative: every listed proof must have an accepted summary, exported bug artifact, replay verdict with `replay_class = snapshot_backed_reproduced`, `reproduced = true`, `command.exit_status = 0`, `replay_parent_depth > 0`, and either a present digest-matching `.snapshot.bin` artifact or a verified `.snapshot.bin.chunks.json` sidecar whose ordered chunks reconstruct to the referenced digest.

This is workload coverage evidence, not a mathematical or universal determinism proof. It only supports claims about the named bounded workload rails and their committed verdict/snapshot artifacts. Operator-facing supported vs experimental status is generated in `docs/replay-readiness-status.md`. New breadth claims should add a manifest entry plus committed evidence and pass:

```bash
cargo run -p chaoscontrol-evidence --bin check-replay-proof-coverage -- .
cargo run -p chaoscontrol-evidence --bin check-replay-proof-coverage -- --check-doc .
cargo run -p chaoscontrol-evidence --bin generate-replay-readiness-report -- --check .
```
