# Fresh replay verdict dogfood receipt

- Run: `raft-verdict-dogfood-20260506-231137`
- Git revision: `e853fd872898719fac7e6984c041924ab9d3e98e`
- Run exit status: `1` (bug found)
- Exported bugs: 2
- Selected replay verdict: `replay-verdict-bug0.json`
- Replay exit status: `1`
- Replay class: `schedule_only_replay_gap`
- Reproduced: `false`
- Replay parent depth: `0`
- Snapshot validation: `valid`, digest verified `true`

## Conclusion

This fresh, direct Raft snapshot-probe dogfood run exercised the Rust-owned `reproduce --verdict-output` path outside the Nix smoke wrapper. The selected bug produced a machine-readable verdict, but the verdict is `schedule_only_replay_gap` rather than accepted snapshot-backed proof because the exported bug has `replay_parent_depth = 0`. This is useful regression evidence for non-happy-path classification and does not overclaim deterministic hypervisor replay.

Raw `run.log` and `reproduce-bug0.log` remain local debug artifacts and are excluded from committed evidence. The large local snapshot artifact is hash-addressed in the bug/verdict but not accepted as proof for this run because parent depth is zero.
