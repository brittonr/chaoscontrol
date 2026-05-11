## Why

Item 8 in the competitor gap list is a FoundationDB-style in-process deterministic simulator. ChaosControl is currently VMM-centered. That is valuable for real guest binaries, but it lacks a fast single-process/discrete-event rail for model-level workloads, simulated network/disk/time, and high-volume fault exploration without launching VMs.

## What Changes

- Add an in-process deterministic simulator domain as an explicit experimental rail.
- Define simulator kernel boundaries for time, RNG, network, disk, tasks, and fault injection.
- Require evidence that simulator results are reproducible and clearly separated from VM replay evidence.

## Capabilities

### New Capabilities
- `in-process-deterministic-simulator`: deterministic single-process simulation rail for selected Rust workloads or models.

## Impact

- **Files**: new simulator crate/module, workload adapter traits, fixtures, evidence models, docs/status surfaces, Nix checks.
- **APIs**: simulator clock/RNG/network/disk/task traits and workload adapter contracts.
- **Testing**: pure deterministic replay tests, negative nondeterminism fixtures, simulator-vs-VM boundary checks, and readiness anti-overclaim gates.
