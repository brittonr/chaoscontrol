# Plan

- [x] **Dogfood it** — Run ChaosControl against a real target (a small Raft implementation, SQLite WAL, etcd). This is where you'll find what's actually missing vs. theoretically complete.

Progress: 1/1 steps completed

## Receipt

- 2026-05-06: Raft dogfood run saved under `dogfood-results/raft-20260506-095025/`. It exercised 42/42 assertion sites and reported one bug schedule, but standalone reproduction did not replay it from `bug_0.json`; next high-ROI fix is bug-report replayability/receipt integrity.
