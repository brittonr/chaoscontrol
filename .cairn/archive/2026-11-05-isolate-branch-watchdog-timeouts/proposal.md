# Isolate Branch Watchdog Timeouts

## Why

A single-vCPU run arms a 100 ms `CLOCK_MONOTONIC` watchdog around each `KVM_RUN` (`vm.rs:196`, `vm.rs:3697`). Host descheduling counts toward that deadline. When a branch's watchdog expires on a loaded host, the controller latches its permanent round poison, as the `deterministic-smp-scheduling` requirement `chaoscontrol.deterministic_smp.controller_poison` requires. After that, every later restore and run on that controller fails. Two explorer paths react differently:

- The sequential path (`--workers 1`, the CLI default) propagates `HostWatchdogTimeout` and aborts the campaign.
- The worker pool turns every branch error into a zero-coverage placeholder. After one watchdog, the poisoned worker converts every later branch it receives into a placeholder without reporting that it did so.

A 2026-09-25 local bug-hunt run shows the sequential abort. The run used seed 42, 10 rounds, 8 branches, 2000 ticks, and one VM at a host load average near 50 on 32 cores. Six of seven Raft variants stopped after 2 to 5 rounds with `Exploration failed: Vm { source: HostWatchdogTimeout { vcpu: 0 } }`. The fire point differed between identical reruns (tick 7211 compared with tick 11210), so the abort depends on host timing rather than on a guest verdict.

The `deterministic-smp-scheduling` requirement `chaoscontrol.deterministic_smp.watchdog` classifies a timeout as operational evidence that has no guest-verdict authority. It allows the shell to retry or stop.

## What Changes

- Treat `HostWatchdogTimeout` from a branch run as an operational branch timeout. Do not treat it as a campaign failure. r[chaoscontrol.branch_watchdog.isolation]
- Replace the poisoned controller with a fresh controller restored from the retained bootstrap state. Do not reuse the poisoned controller. r[chaoscontrol.branch_watchdog.recovery]
- Retry the same branch from its parent snapshot, up to a named attempt bound. Discard the branch after the bound is exhausted. A discarded branch adds no coverage, bugs, corpus or frontier entry, or branch count. r[chaoscontrol.branch_watchdog.retry] r[chaoscontrol.branch_watchdog.discard]
- Apply the same handling in the sequential, worker-pool, and input-tree paths. r[chaoscontrol.branch_watchdog.paths]
- Record watchdog firings and discarded branches per round in `RoundHistory`, the checkpoint round history, metrics lines, logs, and the human report. r[chaoscontrol.branch_watchdog.evidence]
- Fail closed with `HostWatchdogTimeout` when every attempted branch in a round is discarded. r[chaoscontrol.branch_watchdog.fail_closed]
- Keep all other VM errors and the bootstrap path unchanged. r[chaoscontrol.branch_watchdog.isolation]
- Make `scripts/bug-hunt.sh` report an explorer error as an error rather than as a found bug. r[chaoscontrol.branch_watchdog.evidence]

## Impact

- **Code**: new `chaoscontrol-explore` branch-recovery module, explorer round loops, worker-pool branch collection, `RoundHistory`, metrics, report formatting, and `scripts/bug-hunt.sh`.
- **Compatibility**: new `RoundHistory` fields default to zero, so older checkpoints still load.
- **Evidence**: a retried branch starts from the same parent snapshot as an uninterrupted run. At parent depth 3 or more, the pre-existing overlay dirty-page gap can still make the replacement controller's memory differ from the long-lived controller's. A discarded branch changes the frontier trajectory. The report exposes both counts.

## Non-Goals

- No retry of `KVM_RUN` inside the VMM. Re-entry would repeat PIT and TSC synchronization at an instruction boundary chosen by host timing.
- No change to the watchdog interval, controller poison semantics, SMP handling, replay, or minimization.
- No fix for the seed-to-seed coverage variance observed in the same run, or for the overlay dirty-page gap confirmed at replay-parent depth 3 or more. Each needs its own change.
