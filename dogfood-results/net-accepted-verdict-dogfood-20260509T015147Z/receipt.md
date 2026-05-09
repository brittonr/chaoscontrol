# Net accepted snapshot-backed replay proof

## Scope

Fresh bounded networking `snapshot_replay_probe` proof for ChaosControl replay verdict coverage after the current snapshot codec moved to CBOR/zstd. This proves the selected net workload rail only; it is not a mathematical or universal deterministic hypervisor proof.

## Evidence

- git rev: `fbcb2a5a790d7687285a98cc353beb205f3d4843`
- assertion: `3141592653` (`net snapshot replay probe trips only after restored parent context`)
- probe cmdline: `net_bug=snapshot_replay_probe net_snapshot_probe_fail_after=3`
- selected bug: `bug_0.json`
- verdict: `replay-verdict-bug0.json`
- snapshot codec: `simulation-snapshot-cbor-zstd-v2`
- snapshot: `snapshots/0a1d6142c36dd4fc0f875deffdef316e7965c5b310b0b96ec0434789fe047dd5.snapshot.bin`

## Result

Standalone reproduce emitted `snapshot_backed_reproduced` with `reproduced=true`, `replay_parent_depth=1`, valid digest-checked CBOR snapshot evidence, and command exit status 0. Raw run/export/reproduce logs, attempts scratch, and checkpoint payloads are debug-only and excluded from committed evidence.
