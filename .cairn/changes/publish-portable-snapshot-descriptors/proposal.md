## Why

ChaosControl implements exact cohort-bound snapshots and retained snapshot artifacts. Current replay-parent references include a store, digest, codec, schema version, and confined host path, but they are not a public evidence or interchange format.

External consumers need a stable descriptor for identity, complete closure, compatibility preflight, and restore observations without importing host paths or ChaosControl storage authority.

## What Changes

- Add a Rust-owned `chaoscontrol-snapshot-descriptor-v1` public DTO and canonical BLAKE3 identity projection.
- Bind the exact completeness profile, state schema, architecture, KVM capability cohort, CPU and MSR inventory, VM topology, device identities, backend classes, guest artifacts, deterministic-state owners, and payload closure.
- Separate logical snapshot identity and closure from host stores, indexes, confined paths, tickets, and other locator hints.
- Add a bounded closure manifest for monolithic and chunked snapshot artifacts with tagged digest algorithms and exact lengths.
- Add pure descriptor, inventory, topology, closure, and destination-compatibility preflight.
- Add Rust-owned JSON projections plus generated Nickel review contracts and positive and negative fixtures.
- Add detached restore observation receipts for preflight, materialization, mutation start, poison, completion, and bounded continuation checks.
- Publish consumer guidance and one Molten adapter fixture without adopting Molten world-commit semantics.

## Dependencies

- Existing `exact-x86-kvm-v1` snapshot profile and state schema.
- Existing snapshot chunk manifests, replay-parent references, KVM release evidence, and bounded snapshot replay.
- Kamacite only for an optional interchange projection. ChaosControl retains snapshot descriptor meaning.

## Non-Goals

- Cross-architecture, cross-topology, or silent cross-cohort snapshot portability.
- A new snapshot payload codec, object store, replication layer, retention policy, or restore authority system.
- Proof of KVM, guest, kernel, device, storage, or replay correctness.
- Molten world-commit identity, branch policy, semantic merge, or release eligibility.

## Impact

- **Core**: public descriptor DTOs, canonical identity material, closure validation, compatibility decisions, and diagnostics.
- **Shell**: descriptor emission, artifact read-back, destination observation, restore receipts, and consumer export.
- **Contracts**: generated Nickel review contract and versioned positive and negative fixtures.
- **Testing**: valid monolithic and chunked descriptors plus negative incomplete inventory, wrong cohort, topology drift, locator substitution, digest mismatch, missing chunk, unknown algorithm, stale schema, poison, and portability-overclaim cases.
