## Why

ChaosControl already uses Nickel at its VM run-configuration and evidence-review boundaries, but two substantial human-authored configuration families remain Rust/CLI-only:

- `chaoscontrol_evidence::in_process_simulator::SimulatorConfig` carries workload, scheduler, virtual-clock, RNG, network, disk, fault-schedule, artifact, and claim-scope inputs whose invariants are enforced only after JSON reaches Rust.
- `chaoscontrol_explore::{ExplorerConfig, CampaignConfig, SerializableCampaignConfig}` carries seed sets, VM topology, scheduling, exploration, mutation, fault, worker, logging, metrics, and resource bounds assembled from a large CLI surface without an equivalent mergeable authoring contract.

The existing `contracts/evidence/run-config.ncl` also leaves modes, paths, seeds, and several numeric classes broad. This makes long-running or expensive campaigns discover malformed intent late and prevents reusable reviewed profiles from composing safely.

## What Changes

- Add shared Nickel configuration primitives and exact contracts for VM exploration runs, in-process simulator profiles, campaign profiles, and finite fault-schedule descriptors.
- Extend the contract ownership registry so human-authored profiles are Nickel-owned while runtime progress, traces, checkpoints, outcomes, bug reports, and receipts remain Rust-owned.
- Export deterministic JSON profiles at an explicit preparation boundary; Rust deserializes and revalidates them before constructing runtime configs.
- Add cross-field validation for topology, seed uniqueness, scheduler/RNG mode, budgets, worker plans, coverage mode, mutation ranges, fault targets, artifact identities, and claim scope.
- Add positive and negative profile fixtures and source/projection/Rust parity checks.
- Do not evaluate Nickel in the simulator or campaign hot path.

## Impact

- **Authoring surfaces**: `contracts/evidence/run-config.ncl` plus new simulator, campaign, schedule, and shared contract modules.
- **Rust boundaries**: conversion into `SimulatorConfig`, `ExplorerConfig`, and `CampaignConfig`; runtime validators remain mandatory for external JSON.
- **Evidence registry**: new Nickel-authored families and explicit references to the Rust-derived records they configure.
- **Active-change boundary**: fault-attempt/outcome records, assertion identity, snapshot completeness, SMP scheduling, virtio validation, and Wasm exploration remain owned by their active packages. This package validates only pre-run intent and references.
- **Claims**: profile validation does not prove KVM availability, guest correctness, deterministic replay, fault application or observation, campaign completion, or evidence acceptance.
