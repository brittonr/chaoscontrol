## Why

Rust workloads should not need separate conceptual integrations for local in-process simulation and VM/hypervisor replay. The same Rust adapter shape should run locally in simulator mode for fast checks and in VM campaign mode for replay proof, while receipts make the evidence class explicit.

## What Changes

- **Shared Rust adapter trait**: Define the minimum workload adapter surface common to local simulator runs and VM/hypervisor campaigns.
- **Comparable receipts**: Link simulator receipts and VM replay receipts by workload identity, adapter version, seed/fault schedule, and artifact digests.
- **Evidence boundary**: Prevent simulator success from being promoted as VM replay proof.

## Capabilities

### Modified Capabilities
- `rust-workload-harness`: Adds adapter shape shared by simulator and VM paths.
- `in-process-deterministic-simulator`: Links adapter simulator evidence to VM campaign/replay evidence without merging evidence classes.

## Impact

- **Files**: SDK/harness traits, simulator adapter models, evidence receipts, docs/examples.
- **APIs**: New Rust adapter trait/config types likely; no non-Rust SDKs.
- **Dependencies**: None expected.
- **Testing**: Pure adapter tests, simulator receipt tests, VM receipt-link fixtures, negative promotion-gate tests.
