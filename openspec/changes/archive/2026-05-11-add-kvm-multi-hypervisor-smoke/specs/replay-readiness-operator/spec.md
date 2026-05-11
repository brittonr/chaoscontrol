## ADDED Requirements

### Requirement: KVM-backed local multi-hypervisor smoke rail

The system MUST provide a bounded, explicitly KVM-backed local smoke rail for the local multi-hypervisor campaign runner that executes real replay-readiness dogfood commands through at least two local hypervisor worker identities and emits receipt-backed evidence without raw-log scraping or hosted-service claims.

#### Scenario: Smoke rail executes dogfood receipts through local hypervisor workers

- GIVEN a machine with `/dev/kvm` available and a selected bounded workload set
- WHEN the KVM local multi-hypervisor smoke rail runs
- THEN it writes a campaign plan, campaign receipt, queue-state file, per-run replay-readiness receipts, and summary file where every passed run includes a replay-readiness summary from a real dogfood command and every run is linked to a local hypervisor worker identity and durable queue lease

#### Scenario: Smoke rail remains bounded and local

- GIVEN a generated KVM multi-hypervisor smoke receipt or summary
- WHEN an operator reviews the artifact
- THEN it states the evidence covers bounded local KVM multi-hypervisor campaign execution only, not a hosted service, shared remote queue, cross-machine scheduler, fleet-scale throughput, or full Antithesis-style replacement
