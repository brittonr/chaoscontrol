## ADDED Requirements

### Requirement: Local replay scheduler execution receipt [r[local-scheduler-execution-receipt]]

The system MUST provide a bounded local sequential scheduler execution receipt that runs or records multiple scheduler-plan commands, links each successful run to a replay-readiness receipt summary, and does not claim hosted scheduling, fleet-scale workers, shared queues, cross-machine orchestration, raw-log scraping, or product parity.

#### Scenario: Scheduler execution records receipt-backed runs [r[local-scheduler-execution-receipt.records-runs]]

- GIVEN a valid local scheduler receipt with `concurrency=1` and multiple run-plan entries
- WHEN the local scheduler execution command runs the plan
- THEN it writes an execution receipt with one run result per entry, command, exit code, receipt path, stable receipt summary for passed runs, and bounded-local-sequential scope

#### Scenario: Scheduler execution fails closed on overclaims [r[local-scheduler-execution-receipt.fail-closed]]

- GIVEN a scheduler execution receipt that enables raw-log scraping, declares hosted or fleet/shared-queue semantics, duplicates run IDs, omits successful receipt summaries, or records concurrency greater than one
- WHEN the scheduler execution validator runs
- THEN it rejects the receipt and does not accept it as scheduler orchestration evidence

#### Scenario: Scheduler execution is packaged with readiness artifacts [r[local-scheduler-execution-receipt.packaged]]

- GIVEN the replay-readiness Nix check succeeds
- WHEN the check output is inspected
- THEN it contains a non-empty scheduler execution plan, scheduler execution receipt, scheduler execution summary, and linked per-run replay-readiness receipts
