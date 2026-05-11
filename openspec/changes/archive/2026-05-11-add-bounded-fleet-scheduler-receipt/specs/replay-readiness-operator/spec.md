## ADDED Requirements

### Requirement: Bounded fleet scheduler receipt [r[bounded-fleet-scheduler-receipt]]

The system MUST provide a bounded hosted/fleet scheduler receipt that records durable queue entries, worker leases, run outcomes, replay-readiness receipt summaries, and linked operator decision receipts without relying on raw-log scraping or claiming a live always-on product service.

#### Scenario: Fleet scheduler receipt links durable queue and worker runs [r[bounded-fleet-scheduler-receipt.links-queue-workers-runs]]

- GIVEN a fleet scheduler receipt with a durable queue, worker leases, run records, and decision receipt links
- WHEN the fleet scheduler receipt validator runs
- THEN it accepts only receipts where each run links to an existing queue entry, existing worker, receipt path, and replay-readiness summary

#### Scenario: Fleet scheduler receipt rejects ungrounded claims [r[bounded-fleet-scheduler-receipt.rejects-ungrounded-claims]]

- GIVEN a fleet scheduler receipt with raw-log scraping, duplicate identifiers, missing worker links, missing queue links, missing receipt summaries for passed runs, or no linked decision receipts
- WHEN the fleet scheduler receipt validator runs
- THEN it rejects the receipt as insufficient hosted/fleet scheduler evidence

#### Scenario: Fleet scheduler receipt is packaged with readiness output [r[bounded-fleet-scheduler-receipt.packaged]]

- GIVEN the replay-readiness Nix check succeeds
- WHEN the check output is inspected
- THEN it contains a non-empty fleet scheduler receipt and summary alongside local scheduler execution artifacts
