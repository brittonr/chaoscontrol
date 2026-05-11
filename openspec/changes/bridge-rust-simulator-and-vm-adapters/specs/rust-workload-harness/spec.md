## ADDED Requirements

### Requirement: Shared Rust simulator and VM adapter [r[rust-workload-harness.sim-vm-adapter]]

ChaosControl MUST provide a Rust workload adapter shape that can identify and configure the same workload for local in-process simulator runs and VM/hypervisor campaigns while preserving distinct evidence classes.

#### Scenario: Adapter identifies workload across modes [r[rust-workload-harness.sim-vm-adapter.identity]]

- GIVEN a Rust workload implements the shared adapter surface
- WHEN it is run in simulator mode and VM campaign mode
- THEN both receipts record the workload name, adapter version, scenario identity, selected seed or fault schedule reference, and relevant artifact digests
- AND the receipts label simulator-local and VM replay evidence separately

#### Scenario: Adapter rejects unsupported environment hooks [r[rust-workload-harness.sim-vm-adapter.unsupported-hooks]]

- GIVEN a workload adapter uses wall-clock time, host randomness, filesystem/network IO, or VM-only hypercalls without declaring the environment-specific hook
- WHEN the local simulator adapter validation runs
- THEN it rejects the adapter as unsupported simulator evidence without blocking VM-only campaign use
