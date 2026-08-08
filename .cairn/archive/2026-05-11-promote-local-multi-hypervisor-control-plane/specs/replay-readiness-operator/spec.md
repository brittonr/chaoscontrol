## MODIFIED Requirements

### Requirement: Local multi-hypervisor control plane [r[local-multi-hypervisor-control-plane]]

The system MUST provide and identify as supported-bounded-local a single-machine control plane for multiple local ChaosControl hypervisor workers when evidence binds durable queue state, worker resource placement, per-run artifact roots, artifact indexes, bug follow-up jobs, receipt-backed execution, and KVM smoke proof without claiming hosted service, cross-machine fleet scheduling, or full product parity.

#### Scenario: Control plane leases work to local hypervisors [r[local-multi-hypervisor-control-plane.leases-local-workers]]

- GIVEN a local control-plane plan with bounded worker count, queue state path, worker IDs, CPU/memory budgets, per-worker artifact roots, and replay-readiness commands
- WHEN the control plane runs the plan
- THEN it starts or records local hypervisor workers, leases each queue entry to exactly one worker, writes state after each transition, and emits a receipt linking worker, lease, command, exit status, artifact root, and replay-readiness summary

#### Scenario: Control plane schedules reproduce and minimize follow-ups [r[local-multi-hypervisor-control-plane.bug-handoff]]

- GIVEN a local hypervisor run emits a bug artifact with snapshot-backed replay context or schedule-only gap evidence
- WHEN bug follow-up policy is enabled
- THEN the control plane enqueues local reproduce and/or minimize jobs that link back to the original worker/run, bug artifact, snapshot ref, and resulting verdict or minimization receipt

#### Scenario: Control plane is promoted only with local evidence [r[local-multi-hypervisor-control-plane.promoted-local-only]]

- GIVEN the generated replay-readiness status report is rendered from committed evidence
- WHEN the local multi-hypervisor control-plane surface is marked supported
- THEN the row cites the durable receipt model, KVM smoke rail, worker budgets, artifact roots/indexes, queue-state transitions, run receipts, bug follow-up jobs, and local artifact retention
- AND the row states that it is not a hosted service, shared remote queue, cross-machine scheduler, universal fleet-scale throughput claim, or full Antithesis-style product replacement

#### Scenario: Control plane rejects unsafe local coordination [r[local-multi-hypervisor-control-plane.fail-closed]]

- GIVEN a control-plane plan, receipt, or readiness row with duplicate active leases, missing state persistence, missing worker/run links, unbounded worker count, successful runs without receipt summaries, raw-log scraping, remote shared queue semantics, cross-machine scheduling claims, hosted-service language, or missing local evidence tokens
- WHEN the validator or promotion gate runs
- THEN it rejects the artifact as insufficient local multi-hypervisor evidence
