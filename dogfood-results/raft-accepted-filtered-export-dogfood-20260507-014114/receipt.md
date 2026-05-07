# Accepted filtered-export replay verdict dogfood (raft-accepted-filtered-export-dogfood-20260507-014114)

Status: accepted.

This run exercised `scripts/accepted-snapshot-verdict-dogfood.py` after targeted `export-bugs` support landed. The wrapper completed without timeout tolerance: `export-bugs` exited 0 with `--assertion-id 1806003755 --min-replay-parent-depth 1 --max-bugs 1`, exported one snapshot-backed candidate, and standalone reproduce emitted an accepted replay verdict.

## Result

- Replay class: `snapshot_backed_reproduced`
- Reproduced: `True`
- Assertion ID: `1806003755`
- Replay parent depth: `2`
- Snapshot status: `valid`
- Snapshot digest verified: `True`
- Reproduce exit status: `0`

## Coverage

Registered assertions: 43; exercised: 41; passed: 40; failed: 1; unexercised: 2.

## Replay context

parent-snapshot-required: persisted snapshot ref loaded

BUG REPRODUCED — assertion 1806003755 failed

Raw `run.log`, `export-bugs.log`, `reproduce.log`, `checkpoint.json`, and `attempts/` remain debug-only/excluded from git.
