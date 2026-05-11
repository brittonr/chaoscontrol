## ADDED Requirements

### Requirement: Local multi-hypervisor control plane [r[local-multi-hypervisor-control-plane]]

The system MUST provide a bounded single-machine control plane for multiple local ChaosControl hypervisor workers that manages durable queue state, worker resource placement, per-run artifact roots, bug follow-up jobs, and receipt-backed execution without claiming hosted service, cross-machine fleet scheduling, or full product parity.

#### Scenario: Control plane leases work to local hypervisors [r[local-multi-hypervisor-control-plane.leases-local-workers]]

- GIVEN a local control-plane plan with bounded worker count, queue state path, worker IDs, optional CPU/memory budgets, per-worker artifact roots, and replay-readiness commands
- WHEN the control plane runs the plan
- THEN it starts or records local hypervisor workers, leases each queue entry to exactly one worker, writes state after each transition, and emits a receipt linking worker, lease, command, exit status, artifact root, and replay-readiness summary

#### Scenario: Control plane schedules reproduce and minimize follow-ups [r[local-multi-hypervisor-control-plane.bug-handoff]]

- GIVEN a local hypervisor run emits a bug artifact with snapshot-backed replay context or schedule-only gap evidence
- WHEN bug follow-up policy is enabled
- THEN the control plane enqueues local reproduce and/or minimize jobs that link back to the original worker/run, bug artifact, snapshot ref, and resulting verdict or minimization receipt

#### Scenario: Control plane rejects unsafe local coordination [r[local-multi-hypervisor-control-plane.fail-closed]]

- GIVEN a control-plane plan or receipt with duplicate active leases, missing state persistence, missing worker/run links, unbounded worker count, successful runs without receipt summaries, raw-log scraping, remote shared queue semantics, cross-machine scheduling claims, or hosted-service language
- WHEN the validator runs
- THEN it rejects the artifact as insufficient local multi-hypervisor evidence

### Requirement: Local multi-hypervisor artifact hygiene [r[local-multi-hypervisor-artifact-hygiene]]

The local multi-hypervisor control plane MUST keep campaign artifacts content-addressed or hash-bound, bounded by retention policy, and attributable to a worker/run without relying on raw logs.

#### Scenario: Artifact index binds run outputs [r[local-multi-hypervisor-artifact-hygiene.index]]

- GIVEN a completed local multi-hypervisor campaign
- WHEN the artifact index is generated
- THEN it records each worker/run artifact root, replay-readiness receipt path, bug artifact path, snapshot/chunk manifest digest when present, reproduce/minimize receipt paths, and retention/GC status
