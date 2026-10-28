# Redb candidate campaign — 2026-10-28

## Executed

```sh
local-multi-hypervisor-kvm-smoke \
  --workloads redb,rust-workload \
  --out /dev/shm/cc-redb2/run1 \
  --replay-readiness <post-repair replay-readiness>/bin/replay-readiness \
  --scheduler-receipt <replay-readiness-scheduler-receipt>/bin/replay-readiness-scheduler-receipt
```

Exit 0. Receipt:
`status=recorded campaign=local-kvm-smoke-0001 hypervisors=2 runs=2 passed=2
restart_persisted=true workloads=redb,rust-workload
fault_classes=block,network,scheduler,timer
scope=bounded-local-multi-hypervisor`.

Both run receipts are `status=passed` with `failed_phase` absent and
`expectation_status=matched`. The retained files are the campaign plan, campaign
receipt, both per-run receipts, and the console tail.

## Scope

The `redb` workload ran with the block fault class under two local
hypervisors, including restart persistence. This is a bounded local KVM
observation. It does not prove arbitrary Redb or filesystem behavior, Raft
storage recovery, or release readiness.

The requested Molten candidate has no ChaosControl dogfood workload or cohort
in this repository, so no Molten candidate result exists. The Redb half of the
candidate-campaign task is executed; the Molten half remains open for a future
admitted workload cohort.
