# redb accepted snapshot-backed replay verdict dogfood

- workload: redb non-Raft guest
- git rev: `6d4330df982ace86d019aa1aefc21d8f1f45225c`
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
- snapshot digest: `sha256:2258099fe1cde4119b3f5b380456b710bb81962e523f1ea09229a0a4a1564a01`

This refreshes the redb workload proof for the bounded snapshot-backed replay rail under the current CBOR/zstd snapshot codec. It does not claim global hypervisor determinism. Raw logs, checkpoint, duplicate bug exports, and attempt directories are local debug artifacts and intentionally excluded from committed evidence.
