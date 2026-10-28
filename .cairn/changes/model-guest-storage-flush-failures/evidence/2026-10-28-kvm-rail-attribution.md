# KVM rail attribution — 2026-10-28

## Observed full rail

```sh
nix flake check -L --keep-going --no-write-lock-file \
  --option builders '' --max-jobs 2 --cores 2
```

Result: exit 1. Every check except
`checks.x86_64-linux.local-multi-hypervisor-kvm-smoke` built. That check failed
three times with `status=failed ... runs=2 passed=0` for the `raft` and
`rust-workload` dogfood runs. Each run receipt reports `failed_phase=dogfood`
and `run_exit_status=1`; static gates all pass.

A direct reproduction with retained artifacts:

```sh
local-multi-hypervisor-kvm-smoke --out /dev/shm/cc-smoke/run1 \
  --replay-readiness <replay-readiness>/bin/replay-readiness \
  --scheduler-receipt <scheduler>/bin/replay-readiness-scheduler-receipt
```

produced the same `status=failed` receipt, and the raft attempt still exported
ten bugs and reproduced `bug_1` (`exit_status: 0`). The failure is the dogfood
expectation/run status, not a missing campaign execution.

## Attribution

The repository received roughly 260 commits from another actor after this
change's storage work. Two isolated builds of the same check separate the
change from that later work:

| Revision | Contents | Check result |
|---|---|---|
| `ec4fa6c` | before this change | retry 1 `partial passed=1`; retry 2 `recorded passed=2`; exit 0 |
| `473c0c7` | this change's flush, persistence, oracle, schedule, and receipt work | first run `recorded passed=2`; exit 0 |
| `56c33411` (HEAD at observation) | later external lineage plus this change's Raft disk binding | three runs `failed passed=0`; exit 1 |

The later Raft disk-binding commit only adds a pure model module and a
`pub mod` declaration; the guest binary does not call it. The KVM failure is
therefore attributed to the intervening external lineage, not to this change.

## Scope

No external commit was reverted, amended, or re-authored. The failing check
stays recorded as the current repository state. Campaign tasks that require a
passing multi-hypervisor rail remain blocked on that external regression; the
focused model, oracle, schedule, receipt, formatting, Clippy, and unit checks
in this change remain green at `473c0c7` and at HEAD.
