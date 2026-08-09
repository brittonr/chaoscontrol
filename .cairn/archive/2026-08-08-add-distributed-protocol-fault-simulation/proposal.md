## Why

ChaosControl owns deterministic VMM simulation and evidence. It already has a VMM rail and an in-process (FoundationDB-style) simulator for model-level workloads. It does not yet have a dedicated protocol-simulation rail that exercises a distributed protocol under deterministic fault injection and rewinds to reproduce a failing schedule.

Celld runs a deterministic simulation of its distributed protocol under fault injection before each release: ownership leases, replication, and reacquisition are explored under injected faults, and the schedule is replayed to reproduce a finding. That is a clean, transferable approach for ChaosControl: seam a distributed protocol into a deterministic simulation, inject bounded faults, and prove a single failing schedule reproduces.

ChaosControl should own this as a bounded, evidence-gated rail. It does not claim to prove arbitrary protocol correctness.

## What Changes

- Add a distributed-protocol simulation rail for adapter-based protocols.
- Define fault-injection hooks for node loss, message loss, reorder, duplication, and partition.
- Require a deterministic seed and schedule, and a single-schedule replay proof.
- Keep protocol-simulation evidence separate from VMM and in-process simulator evidence.
- Add positive, negative, and boundary fixtures for reproduce, nondeterminism, and overclaim.
- Reference the reviewed Celld protocol-simulation approach as a bounded, non-parity input.

## Capabilities

### New Capabilities
- `distributed-protocol-simulation`: deterministic replayable simulation of an adapter-based distributed protocol under injected faults.

## Impact

- **Files**: protocol-simulation crate/module, adapter contract, fault hooks, fixtures, evidence models, Nix checks.
- **Apis**: protocol adapter, fault schedule, seed, and single-schedule replay contract.
- **Testing**: pure deterministic replay tests, negative nondeterminism fixtures, fault-cover matrix, and anti-overclaim gates.

## Non-goals

- Do not claim arbitrary distributed-protocol correctness.
- Do not claim parity with, or equivalence to, the Celld distributed protocol.
- Do not treat protocol-simulation evidence as VM replay proof.
- Do not add a general model checker or a new transport.
