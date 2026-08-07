# Replay Proof Coverage

The manifest retains historical snapshot-backed replay artifacts. Current assertion-identity admission promotes 0 workload(s) and blocks 4 workload(s).

| Workload | Assertion ID | Evidence | Verdict |
| --- | ---: | --- | --- |
| Raft | `1806003755` | `dogfood-results/raft-accepted-verdict-dogfood-20260509T030143Z/` | `blocked-assertion-identity` |
| redb | `2718281828` | `dogfood-results/redb-accepted-verdict-dogfood-20260510T191449Z/` | `blocked-assertion-identity` |
| net | `3141592653` | `dogfood-results/net-accepted-verdict-dogfood-20260509T015147Z/` | `blocked-assertion-identity` |
| rust-workload | `1414213562` | `dogfood-results/rust-workload-accepted-verdict-dogfood-20260511T163054Z/` | `blocked-assertion-identity` |

A promoted proof requires an accepted summary, exported bug artifact, replay verdict, retained snapshot, and accepted v2 assertion summary. The v2 summary must bind the selected alias to one admitted structured descriptor. Historical bare-array assertion files remain diagnostic-only.

This is workload coverage evidence, not a mathematical or universal determinism proof. A blocked row does not support a current bounded replay claim. Fresh admitted KVM evidence must pass:

```bash
cargo run -p chaoscontrol-evidence --bin check-replay-proof-coverage -- .
cargo run -p chaoscontrol-evidence --bin check-replay-proof-coverage -- --check-doc .
cargo run -p chaoscontrol-evidence --bin generate-replay-readiness-report -- --check .
```
