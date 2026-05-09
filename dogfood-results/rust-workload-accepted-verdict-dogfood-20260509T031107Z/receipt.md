# rust-workload accepted snapshot-backed replay verdict dogfood

- workload: Rust workload harness sample guest
- git rev: `9c37700539f58b553227ac1effd0bfbd224cba7c`
- assertion: `1414213562` (`rust workload snapshot replay probe trips only after restored parent context`)
- probe cmdline: `rust_workload_bug=snapshot_replay_probe rust_workload_snapshot_probe_fail_after=25`
- selected bug: `bug_0.json`
- verdict: `replay-verdict-bug0.json`
- replay class: `snapshot_backed_reproduced`
- reproduced: `true`
- replay parent depth: `2`
- snapshot codec: `simulation-snapshot-cbor-zstd-v2`
- snapshot status: `valid`
- snapshot digest verified: `true`
- snapshot digest: `sha256:6c703b80417d94ae71f4cc36b41479c66eecd3d5d63d0dcd987542e8e3563199`

This refreshes the Rust workload harness proof for the bounded snapshot-backed replay rail under the current CBOR/zstd snapshot codec. It does not claim global hypervisor determinism. Raw logs, checkpoint, duplicate bug exports, and attempt directories are local debug artifacts and intentionally excluded from committed evidence.
