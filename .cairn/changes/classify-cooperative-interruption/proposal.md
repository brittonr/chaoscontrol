# Change: Classify cooperative interruption

## Why

ChaosControl has useful first-signal behavior. It finishes a safe unit, saves available progress, builds a partial report, and stops new campaign work.

The current names merge three different facts. A received signal is only a request. A completed checkpoint and report form the cooperative terminal outcome. A second signal forces process exit without cleanup evidence.

The signal handler also calls `std::process::exit` while its comment claims that the handler uses only async-signal-safe operations. That guarantee is not established.

## What Changes

- Define request, boundary, finalization, cooperative completion, failure, and forced-exit terms. r[chaoscontrol.exploration_interruption.terms]
- Treat the first SIGINT or SIGTERM as a non-terminal stop request. r[chaoscontrol.exploration_interruption.request]
- Publish terminal interruption only after boundary completion and required finalization. r[chaoscontrol.exploration_interruption.completion]
- Use an async-signal-safe immediate-exit path for a repeated signal. Publish no cleanup claim for that path. r[chaoscontrol.exploration_interruption.forced_exit]
- Keep lifecycle decisions pure and leave signals, process exit, persistence, and reporting in shells. r[chaoscontrol.exploration_interruption.boundary]
- Preserve the distinction between harness interruption and guest `ProcessKill` or `ProcessRestart` faults. r[chaoscontrol.exploration_interruption.scope]
- Add positive and negative subprocess, checkpoint, report, campaign, and fault-scope fixtures. r[chaoscontrol.exploration_interruption.verification]

## Impact

- **Affected spec:** new `exploration-interruption` capability.
- **Affected code:** signal handling, explorer and campaign stop paths, finish classification, reports, and tests.
- **Affected docs:** glossary, operator guidance, report semantics, and lifecycle non-claims.

## Dependencies

- Existing fault-stage evidence remains authoritative for guest process faults.
- Existing storage-recovery work remains authoritative for workload durability and recovery claims.

## Non-Goals

- Do not use forced exit as the normal quit path.
- Do not claim that a forced exit saved a checkpoint, emitted a report, or released resources.
- Do not claim arbitrary application recovery from a guest restart.
- Do not change `ProcessKill`, `ProcessRestart`, or storage faults into harness cancellation.
