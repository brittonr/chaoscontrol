# raft accepted snapshot-backed replay verdict dogfood

- workload: Raft guest
- git rev: `87576cd7aa43fda8326ae0899d8942b92034f192`
- assertion: `1806003755` (`snapshot replay probe trips only after restored parent context`)
- probe cmdline: `raft_bug=snapshot_replay_probe raft_snapshot_probe_fail_after=25`
- selected bug: `bug_0.json`
- verdict: `replay-verdict-bug0.json`
- replay class: `snapshot_backed_reproduced`
- reproduced: `true`
- replay parent depth: `2`
- snapshot codec: `simulation-snapshot-cbor-zstd-v2`
- snapshot status: `valid`
- snapshot digest verified: `true`
- snapshot digest: `sha256:cc0161208b3e591ef79625c902e7418aa70a2f33e1445095790e5202265511d2`

This refreshes the Raft workload proof for the bounded snapshot-backed replay rail under the current CBOR/zstd snapshot codec. It does not claim global hypervisor determinism. Raw logs, checkpoint, duplicate bug exports, and attempt directories are local debug artifacts and intentionally excluded from committed evidence. The snapshot is chunked because the raw artifact exceeds the repository dogfood artifact size guard.
