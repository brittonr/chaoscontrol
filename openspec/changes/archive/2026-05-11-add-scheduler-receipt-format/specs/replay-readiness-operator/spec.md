## ADDED Requirements

### Requirement: Local replay scheduler receipt [r[local-scheduler-receipt]]

The system MUST provide a bounded local replay scheduler receipt format that records a manual multi-run replay-readiness plan without claiming hosted service, fleet-scale scheduler, shared queue, cross-machine orchestration, raw-log scraping, or full product parity.

#### Scenario: Scheduler receipt validates local run plans [r[local-scheduler-receipt.validate]]

- GIVEN a scheduler receipt with schema version, local scope, bounded schedule, non-empty run plan, unique run IDs, workload names, commands, receipt paths, decision policies, and anti-claims
- WHEN the scheduler receipt validator runs
- THEN it returns a stable summary containing recorded status, run count, workloads, schedule mode, and bounded-local-not-hosted scope token

#### Scenario: Unsafe scheduler receipt fails closed [r[local-scheduler-receipt.fail-closed]]

- GIVEN a scheduler receipt that enables raw-log scraping, duplicates run IDs, exceeds max runs, uses unsupported schedule modes, uses unsupported decision policies, or weakens the not-hosted/not-fleet/not-shared-queue anti-claim language
- WHEN the scheduler receipt validator runs
- THEN it exits nonzero and does not accept the receipt as scheduler orchestration evidence

#### Scenario: Scheduler receipt is packaged with readiness artifacts [r[local-scheduler-receipt.packaged]]

- GIVEN the replay-readiness Nix check succeeds
- WHEN the check output is inspected
- THEN it contains a non-empty scheduler receipt and scheduler receipt summary alongside the readiness receipt, dashboard, runbook, fleet index, and decision receipt artifacts
