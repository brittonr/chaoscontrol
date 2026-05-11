## ADDED Requirements

### Requirement: Hosted scheduler shared-state receipt [r[hosted-scheduler-shared-state-receipt]]

The system MUST provide a bounded hosted/fleet scheduler shared-state receipt model that coordinates replay-readiness queue entries across multiple machine identities without relying on raw-log scraping or claiming SaaS/product parity.

#### Scenario: Shared queue leases bind machine and run evidence [r[hosted-scheduler-shared-state-receipt.binds-leases-runs]]

- GIVEN a shared scheduler plan with at least two machine identities, queue entries, bounded lease duration, and replay-readiness commands
- WHEN the hosted/fleet scheduler harness executes or validates the plan
- THEN the receipt records each queue entry, lease ID, lease epoch, machine ID, hypervisor worker ID, command, exit status, receipt path, and stable replay-readiness summary for every passed run

#### Scenario: Shared scheduler rejects ambiguous ownership [r[hosted-scheduler-shared-state-receipt.rejects-ambiguous-ownership]]

- GIVEN a shared scheduler plan or receipt with duplicate queue ownership, stale lease epochs, missing machine links, missing receipt summaries for passed runs, unbounded concurrency, raw-log scraping, or anti-claims that imply full hosted product parity
- WHEN the shared scheduler validator runs
- THEN it rejects the artifact as insufficient hosted/fleet scheduler evidence

### Requirement: Shared replay-readiness decision store [r[shared-replay-readiness-decision-store]]

The system MUST provide a bounded shared decision-store receipt model that records operator decisions against replay-readiness receipts with writer identity, revision/conflict evidence, source receipt links, and explicit non-claims for a production hosted UI unless separately evidenced.

#### Scenario: Shared decision records preserve conflict-aware triage evidence [r[shared-replay-readiness-decision-store.records-decisions]]

- GIVEN replay-readiness run receipts and a shared decision-store state
- WHEN operator decisions are written through the decision-store adapter
- THEN each decision record includes decision ID, writer identity, revision, action, target receipt/run/bug references, source receipt paths, and a stable summary suitable for fleet triage review

#### Scenario: Shared decision store rejects stale or split-brain writes [r[shared-replay-readiness-decision-store.rejects-stale-writes]]

- GIVEN decision records with stale revisions, duplicate decision IDs, conflicting writer updates, missing source receipts, raw-log scraping, or hosted-UI/product-parity overclaims
- WHEN the shared decision-store validator runs
- THEN it rejects the artifact before hosted/fleet triage can be promoted

### Requirement: Hosted scheduler readiness promotion gate [r[hosted-scheduler-readiness-promotion-gate]]

The replay-readiness promotion gate MUST fail closed unless hosted/fleet triage and replay scheduler readiness rows are backed by shared queue and decision-store receipts that prove bounded cross-machine coordination semantics.

#### Scenario: Generated readiness remains unpromoted without shared-state evidence [r[hosted-scheduler-readiness-promotion-gate.unpromoted-without-shared-state]]

- GIVEN only local decision receipts, local scheduler execution, local fleet runtime receipts, local multi-hypervisor campaign receipts, or local KVM smoke evidence
- WHEN the promotion gate evaluates generated readiness status
- THEN hosted/fleet triage UI and cross-machine scheduler claims remain unpromoted and the report names the missing shared queue and shared decision-store evidence

#### Scenario: Promotion requires shared-state artifacts [r[hosted-scheduler-readiness-promotion-gate.requires-artifacts]]

- GIVEN a generated readiness report that promotes hosted/fleet triage or scheduler support
- WHEN the promotion gate runs without a valid shared scheduler receipt, shared decision-store receipt, state snapshot, and linked run receipts
- THEN it exits nonzero and reports the missing hosted/shared-state evidence
