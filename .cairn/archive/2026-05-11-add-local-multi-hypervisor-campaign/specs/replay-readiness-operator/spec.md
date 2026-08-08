## ADDED Requirements

### Requirement: Local multi-hypervisor campaign runner [r[local-multi-hypervisor-campaign-runner]]

The system MUST provide a bounded local campaign runner that can launch multiple ChaosControl hypervisor workers from one durable campaign queue, lease work to those workers, persist queue state after each lease/run transition, and emit receipt-backed evidence without claiming hosted service, cross-machine scheduler, or full product parity.

#### Scenario: Campaign runner links hypervisors to queue leases and run receipts [r[local-multi-hypervisor-campaign-runner.links-hypervisors-leases-runs]]

- GIVEN a local campaign plan with a campaign ID, bounded hypervisor worker count, durable queue state path, workload/run entries, replay-readiness receipt paths, and linked local decision receipt policy
- WHEN the local multi-hypervisor campaign runner executes the plan
- THEN it writes a campaign/fleet receipt where every run links to an existing queue entry, a unique lease, a concrete `hypervisor_worker_id`, command/run metadata, exit status, receipt path, replay-readiness summary for passed runs, and queue-state persistence evidence

#### Scenario: Campaign runner preserves bounded local scope [r[local-multi-hypervisor-campaign-runner.scope]]

- GIVEN a generated campaign receipt or readiness status row for local multi-hypervisor execution
- WHEN an operator reviews the artifact
- THEN it states that the evidence covers bounded local multi-hypervisor campaign execution only, not hosted service, shared remote queue, cross-machine scheduling, universal fleet-scale throughput, or full Antithesis-style product replacement

#### Scenario: Campaign runner fails closed on ambiguous coordination [r[local-multi-hypervisor-campaign-runner.fail-closed]]

- GIVEN a campaign plan or receipt with duplicate leases, duplicate run IDs, missing hypervisor worker links, missing queue links, missing persisted queue-state evidence, successful runs without replay-readiness summaries, raw-log scraping, unbounded worker count, or anti-claims that imply hosted/shared multi-machine scheduling
- WHEN the campaign runner or validator runs
- THEN it rejects the artifact as insufficient multi-hypervisor campaign evidence

#### Scenario: Campaign runner packages replay-readiness evidence [r[local-multi-hypervisor-campaign-runner.packaged]]

- GIVEN the replay-readiness Nix check or equivalent local readiness package succeeds
- WHEN the check output is inspected
- THEN it contains a non-empty local multi-hypervisor campaign plan, campaign/fleet receipt, campaign summary, persisted queue-state proof, and linked per-hypervisor replay-readiness receipts alongside the existing fleet scheduler artifacts
