# Replay Proof Coverage

The manifest retains historical snapshot-backed replay artifacts. Current execution-schema and assertion-identity admission promotes 0 workload(s) and blocks 4 workload(s).

| Workload | Assertion ID | Evidence | Verdict |
| --- | ---: | --- | --- |
| Raft | `3463273124` | `dogfood-results/raft-fresh-v2-proof-20260809/` | `blocked-execution-schema` |
| redb | `4149728441` | `dogfood-results/redb-fresh-v2-proof-20260809/` | `blocked-execution-schema` |
| net | `2074476939` | `dogfood-results/net-fresh-v2-proof-20260809/` | `blocked-execution-schema` |
| rust-workload | `3143219316` | `dogfood-results/rust-workload-fresh-v2-proof-20260809/` | `blocked-execution-schema` |

A promoted proof requires an accepted summary, exported bug artifact, replay verdict, retained snapshot, and accepted v2 assertion summary. The v2 summary must bind the selected alias to one admitted structured descriptor. Historical bare-array assertion files remain diagnostic-only.

`blocked-execution-schema` retains the historical artifacts on disk, but the current typed contract does not decode their superseded verdict or snapshot codecs, so they are not executable, restorable or promotable.

This is workload coverage evidence, not a mathematical or universal determinism proof. A blocked row does not support a current bounded replay claim. Fresh admitted KVM evidence must pass:

```bash
cargo run -p chaoscontrol-evidence --bin check-replay-proof-coverage -- .
cargo run -p chaoscontrol-evidence --bin check-replay-proof-coverage -- --check-doc .
cargo run -p chaoscontrol-evidence --bin generate-replay-readiness-report -- --check .
```
