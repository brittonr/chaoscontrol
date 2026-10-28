# Proposal: Publish a native-process record/replay profile

## Why

Molten's world-replay boundary may attach an exact ChaosControl
native-process replay descriptor as detached diagnostic evidence, but
ChaosControl has not published a producer profile for such a descriptor. Molten
therefore cannot implement the optional member without inventing private
producer semantics.

## What Changes

- Publish `chaoscontrol.native-process-replay-profile.v1` and
  `chaoscontrol.native-process-descriptor.v1` in `chaoscontrol-protocol`.
- Define bounded, canonical native-process records: stable process key,
  generation, lifecycle sequence, executable BLAKE3, logical ticks, exit
  classification, and BLAKE3 evidence refs.
- Validate profile identity, record order, bounds, identity references, and the
  fixed non-claim set.
- Keep descriptors diagnostic-only: they are not semantic observations,
  logical world roots, authority, or equivalence proofs.

## Impact

- **Core**: `crates/chaoscontrol-protocol/src/native_process.rs` and its tests.
- **Consumers**: Molten `harden-world-replay-observation-boundary` can attach a
  descriptor as an opaque diagnostic member and validate its identity.

## Non-goals

- No replay execution, guest instrumentation, or host process spawning.
- No claim that a descriptor reproduces a run or proves process behaviour.
- No authority, repair, voting, or release decision from a descriptor.
