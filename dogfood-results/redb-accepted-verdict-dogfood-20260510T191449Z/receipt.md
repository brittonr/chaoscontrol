# redb accepted snapshot-backed replay verdict dogfood

- workload: redb non-Raft guest
- git rev: `6698ddf636bea9d10d38d332f6d82b9a2376d21a`
- assertion: `2718281828` (`redb snapshot replay probe trips only after restored parent context`)
- probe cmdline: `redb_bug=snapshot_replay_probe redb_snapshot_probe_fail_after=25`
- selected bug: `bug_0.json`
- verdict: `replay-verdict-bug0.json`
- replay class: `snapshot_backed_reproduced`
- reproduced: `true`
- replay parent depth: `1`
- snapshot codec: `simulation-snapshot-cbor-zstd-v2`
- snapshot status: `valid`
- snapshot digest verified: `true`
- snapshot digest: `sha256:bacc336ca613083d1276472e79fe6845220205c30582dbac93cd9537629134ac`

This refreshes the redb workload proof for the bounded snapshot-backed replay rail under the current CBOR/zstd snapshot codec. It does not claim global hypervisor determinism. Raw logs, checkpoint, duplicate bug exports, and attempt directories are local debug artifacts and intentionally excluded from committed evidence.
