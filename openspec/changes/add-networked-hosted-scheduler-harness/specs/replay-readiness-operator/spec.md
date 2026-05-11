## MODIFIED Requirements

### Requirement: Hosted scheduler shared-state receipt [r[hosted-scheduler-shared-state-receipt]]

The system MUST provide a bounded hosted/fleet scheduler shared-state receipt model that coordinates replay-readiness queue entries across multiple machine identities and can be backed by a networked or multi-process harness without relying on raw-log scraping or claiming SaaS/product parity.

#### Scenario: Shared queue leases bind independently started worker evidence [r[hosted-scheduler-shared-state-receipt.networked-workers]]

- GIVEN a hosted scheduler plan with at least two independently started machine/worker identities attached to a shared queue and decision-store adapter
- WHEN the networked hosted scheduler harness executes the plan
- THEN the receipt records each queue entry, lease ID, lease epoch, owning machine ID, hypervisor worker ID, worker session ID, command, exit status, receipt path, stable replay-readiness summary, and state snapshot digest for every completed run

#### Scenario: Networked scheduler rejects ambiguous coordination [r[hosted-scheduler-shared-state-receipt.networked-fail-closed]]

- GIVEN a networked scheduler receipt with duplicate active leases, missing worker-session heartbeats, stale queue-state revisions, missing decision-store revisions, missing receipt summaries for passed runs, raw-log scraping, or anti-claims that imply full hosted product parity
- WHEN the shared scheduler validator runs
- THEN it rejects the artifact as insufficient hosted/fleet scheduler evidence

### Requirement: Shared replay-readiness decision store [r[shared-replay-readiness-decision-store]]

The system MUST provide a bounded shared decision-store receipt model that records operator decisions against replay-readiness receipts with writer identity, revision/conflict evidence, source receipt links, and explicit non-claims for a production hosted UI unless separately evidenced.

#### Scenario: Networked decision writes preserve revision ordering [r[shared-replay-readiness-decision-store.networked-revisions]]

- GIVEN multiple independently started workers writing decision records through the shared decision-store adapter
- WHEN the harness records decisions for run receipts
- THEN each decision record includes writer identity, machine identity, worker session ID, monotonic revision, previous revision or conflict marker, target run/queue entry, source receipt paths, and a stable summary suitable for fleet triage review

### Requirement: Hosted scheduler readiness promotion gate [r[hosted-scheduler-readiness-promotion-gate]]

The replay-readiness promotion gate MUST fail closed unless hosted/fleet triage and replay scheduler readiness rows are backed by shared queue and decision-store receipts that prove bounded cross-machine or multi-process coordination semantics.

#### Scenario: Promotion requires networked shared-state artifacts [r[hosted-scheduler-readiness-promotion-gate.requires-networked-artifacts]]

- GIVEN a generated readiness report that promotes hosted/fleet triage or scheduler support beyond bounded loopback shared-state evidence
- WHEN the promotion gate runs without a valid networked scheduler receipt, worker-session records, queue-state snapshots, decision-store snapshots, and linked run receipts
- THEN it exits nonzero and reports the missing networked hosted/shared-state evidence
