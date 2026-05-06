# Accepted Snapshot Replay Verdict Dogfood: raft-accepted-verdict-dogfood-20260506-232924

## Summary

This direct Raft snapshot-probe dogfood run produced a first-class Rust-owned replay verdict outside the Nix smoke wrapper. The selected bug `bug_2.json` has `replay_parent_depth = 2` and a durable `replay_parent_snapshot_ref`; standalone reproduce loaded the saved parent snapshot and emitted `replay_class = snapshot_backed_reproduced` with `reproduced = true`.

This is bounded evidence for the snapshot-backed Raft replay rail, not a global deterministic-hypervisor proof.

## Commands

- `python scripts/accepted-snapshot-verdict-dogfood.py --explore target/debug/chaoscontrol-explore --kernel /nix/store/8ki40sdb3cb8zyg7zsvjlmybgir46z6b-chaoscontrol-vmlinux/vmlinux --initrd /nix/store/qivwcxa35pmx138c39q23c9vh07i81ss-chaoscontrol-initrd-raft --output dogfood-results/raft-accepted-verdict-dogfood-20260506-232924 --max-attempts 6`
- `target/debug/chaoscontrol-explore reproduce --kernel /nix/store/8ki40sdb3cb8zyg7zsvjlmybgir46z6b-chaoscontrol-vmlinux/vmlinux --initrd /nix/store/qivwcxa35pmx138c39q23c9vh07i81ss-chaoscontrol-initrd-raft --bug dogfood-results/raft-accepted-verdict-dogfood-20260506-232924/bug_2.json --vms 3 --memory-mb 128 --extra-cmdline raft_bug=snapshot_replay_probe raft_snapshot_probe_fail_after=25 --verdict-output dogfood-results/raft-accepted-verdict-dogfood-20260506-232924/replay-verdict-bug2.json`

## Results

- run exit status: 1 (bug found)
- export-bugs status: 124 after selected bug artifacts were written; selected snapshot digest was independently verified before reproduce
- reproduce status: 0 — BUG REPRODUCED — assertion 1806003755 failed
- verdict: `snapshot_backed_reproduced`
- snapshot status: `valid`, digest verified: `True`
- assertion coverage: 42/43 exercised

## Raw artifact policy

`run.log`, `export-bugs.log`, `reproduce-bug2.log`, and `checkpoint.json` are local debug artifacts and intentionally excluded from committed evidence. The committed boundary is the concise receipt, checkpoint summary, selected bug, replay verdict, assertion summary, statuses, hashes, run config, and the content-addressed snapshot artifact required to validate the snapshot ref.
