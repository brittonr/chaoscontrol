# Raft campaign rerun and safety replay

Date: 2026-11-05. Built guests from the current worktree:

- kernel: `/nix/store/5aya7zvynanjw5mh3xvz40sga3zfsmn8-chaoscontrol-vmlinux/vmlinux` (`.#kcov-vmlinux`)
- initrd: `/nix/store/ymfxhvyv4zr3la0q4k6h6famb9avz8y9-chaoscontrol-initrd-raft` (`.#initrd-raft`)

Explorer binary: `target/release/chaoscontrol-explore` built from this worktree.

## Campaigns

| Variant | Params | Bugs | Distinct failing | Result |
| --- | --- | --- | --- | --- |
| control (`raft_bug=none`) | seed 42, 1 VM, 10 rounds × 8 branches × 2000 ticks, bootstrap 5000 | 0 | 0 | no bug files; previously 58 `sometimes` records became bugs |
| `raft_bug=fig8_commit` | same | 0 | 0 | no safety failure observed in this bounded run |
| `raft_bug=premature_commit` | seed 42, 1 VM, 6 rounds × 8 branches × 2000 ticks, bootstrap 5000 | 4 | 1 | one campaign liveness gap recorded as a coverage gap, not a bug |

The control campaign directly exercises the change: the 2026-09-25 baseline reported
58 `sometimes` bugs (leader elected, value committed). This run reports zero bug
records, zero distinct safety failures, and no liveness gap because at least one
branch satisfied each tracked liveness assertion.

## Safety replay

`reproduce` over `premature_commit/bug_0.json` with the same kernel, initrd, seed,
1 VM, bootstrap 5000, and `raft_bug=premature_commit`:

```text
replay_class = snapshot_backed_reproduced
reproduced   = true
snapshot     = valid
diagnostic   = BUG REPRODUCED — assertion 2060528323 failed
verdict      = /tmp/cc-campaign/premature_commit/replay-verdict.json
```

The bug artifact and its replay-parent snapshot are produced by the same run and
retained in the temporary campaign directory. An older committed safety bug
(`raft-fresh-v2-proof-20260809/bug_4.json`) could not be replayed: its snapshot CBOR
predates the current decoder (`missing field failure`), and the older
`raft-accepted-verdict-dogfood-20260509T030143Z/bug_0.json` predates catalog-bound
bug identity. That is a retained-artifact compatibility gap, not a replay result.

## Boundaries

These are bounded campaigns for one kernel, initrd, seed, host, and configuration.
They do not establish universal liveness, correctness, or determinism. The kernel
used is the KCOV-enabled build, not the original non-KCOV build recorded by older
proofs. Replay evidence is limited to the reproduced artifact above.
