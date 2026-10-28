## Tasks

- [x] [serial] Confirm the fatal sequential watchdog path, the permanent controller poison, the worker-pool placeholder conversion, and the 2026-09-25 bug-hunt aborts. r[chaoscontrol.branch_watchdog.isolation]
- [x] [depends:branch-watchdog-baseline] Add the pure branch-recovery classification, attempt policy, and round ledger. r[chaoscontrol.branch_watchdog.isolation] r[chaoscontrol.branch_watchdog.retry] r[chaoscontrol.branch_watchdog.fail_closed]
- [x] [depends:branch-watchdog-core] Add controller replacement from the retained bootstrap state with the base-memory contract. r[chaoscontrol.branch_watchdog.recovery]
- [x] [depends:branch-watchdog-recovery] Route the sequential, input-tree, and worker-pool branch paths through recovery, retry, and discard. r[chaoscontrol.branch_watchdog.paths] r[chaoscontrol.branch_watchdog.discard]
- [x] [depends:branch-watchdog-paths] Record per-round firings and discards in round history, metrics, and the report. Make `scripts/bug-hunt.sh` report explorer errors as errors. r[chaoscontrol.branch_watchdog.evidence]
- [x] [parallel] Add positive and negative tests for classification, attempt policy, the all-discard guard, old-checkpoint defaults, and report output. r[chaoscontrol.branch_watchdog.isolation] r[chaoscontrol.branch_watchdog.retry] r[chaoscontrol.branch_watchdog.fail_closed] r[chaoscontrol.branch_watchdog.evidence]
- [x] [depends:branch-watchdog-tests] Run focused explorer tests and clippy, rerun the Raft bug hunt on a loaded host, and run Cairn validation. r[chaoscontrol.branch_watchdog.recovery] r[chaoscontrol.branch_watchdog.paths]
