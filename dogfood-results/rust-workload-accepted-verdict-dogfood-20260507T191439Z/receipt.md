# Rust workload accepted snapshot verdict dogfood receipt

- run: `dogfood-results/rust-workload-accepted-verdict-dogfood-20260507T191439Z`
- command: `nix run .#rust-workload-accepted-verdict-dogfood -- --output dogfood-results/rust-workload-accepted-verdict-dogfood-20260507T191439Z`
- workload: `rust-workload`
- assertion: `1414213562`
- selected bug: `bug_2.json` at replay parent depth `2`
- export-bugs: `exit_status=0`
- reproduce: `exit_status=0` — BUG REPRODUCED — assertion 1414213562 failed
- verdict: `snapshot_backed_reproduced`, `reproduced=true`, snapshot digest verified

This is bounded workload evidence only. Raw run/reproduce logs and full checkpoint were generated for local debugging and removed from the committed acceptance boundary; `checkpoint-summary.json` and hashes bind the curated evidence.
