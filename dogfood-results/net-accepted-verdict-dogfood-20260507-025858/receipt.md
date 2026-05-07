# Net accepted snapshot-backed replay proof

## Scope

Bounded networking `snapshot_replay_probe` proof for ChaosControl replay verdict coverage. This proves the selected net workload rail only; it is not a mathematical or universal deterministic hypervisor proof.

## Evidence

- git rev: `23eddfbc25073634ad1ad5d5cb2dc2dab57476b0`
- assertion: `3141592653` (`net snapshot replay probe trips only after restored parent context`)
- probe cmdline: `net_bug=snapshot_replay_probe net_snapshot_probe_fail_after=8`
- selected bug: `bug_0.json`
- verdict: `replay-verdict-bug0.json`
- snapshot: `snapshots/062dfe88dabf701842d02ba0524751ad10b5339a533af50c99f5972eedaa3720.snapshot.bin`

## Result

Standalone reproduce emitted `snapshot_backed_reproduced` with `reproduced=true`, `replay_parent_depth=1`, valid digest-checked snapshot evidence, and command exit status 0. Raw run/export/reproduce logs and checkpoint payloads are debug-only and excluded from committed evidence.
