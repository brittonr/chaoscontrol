# redb accepted snapshot-backed replay verdict dogfood

- workload: redb non-Raft guest
- git rev: `b0ee3c7bd4e614fdeaa77da073fd4f7906cb7b3a`
- assertion: `2718281828` (`redb snapshot replay probe trips only after restored parent context`)
- probe cmdline: `redb_bug=snapshot_replay_probe redb_snapshot_probe_fail_after=25`
- selected bug: `bug_0.json`
- verdict: `replay-verdict-bug0.json`
- replay class: `snapshot_backed_reproduced`
- reproduced: `true`
- replay parent depth: `1`
- snapshot status: `valid`
- snapshot digest verified: `true`

This is a second-workload proof for the snapshot-backed replay rail. It does not claim global hypervisor determinism. Raw logs, checkpoint, and attempt directories are local debug artifacts and intentionally excluded from committed evidence.
