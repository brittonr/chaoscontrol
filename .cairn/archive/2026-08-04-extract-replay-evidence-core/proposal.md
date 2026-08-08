## Why

Replay evidence DTOs are split across `chaoscontrol-explore` and `chaoscontrol-evidence`: replay verdicts, artifact hashes, bug/snapshot references, replay parent snapshot references, replay classes, and validation-facing status fields. That duplication makes evidence gates drift from the artifacts the explorer emits.

ChaosControl needs one Rust-owned pure core for replay/evidence DTOs and validators, while keeping VM execution, filesystem traversal, dogfood orchestration, and Nickel boundary contracts in their current shell layers.

## What Changes

- Introduce a shared replay/evidence core crate for replay verdict, artifact hash, snapshot ref, replay parent snapshot ref, replay class, and validation status DTOs.
- Move pure validation and classification logic out of `chaoscontrol-explore` and `chaoscontrol-evidence` into the shared core.
- Keep runtime record serialization Rust-owned; keep Nickel contracts as review-boundary validators rather than runtime trace owners.
- Add compatibility adapters so existing explorer outputs and evidence gates retain public JSON field names during migration.
- Add positive fixtures for current emitted verdicts and accepted evidence receipts.
- Add negative fixtures for malformed hashes, missing snapshot refs, path escapes, unsupported replay classes, stale artifact hashes, and overclaim wording.

## Capabilities

### Modified Capabilities
- `replay-verdicts`: Adds a shared DTO/validation ownership boundary for replay verdict artifacts.
- `rust-owned-evidence-readiness`: Adds the Rust-owned core extraction path for replay evidence validation.

## Impact

- **Files**: OpenSpec deltas now; implementation likely adds a small `chaoscontrol-replay-evidence-core` crate and migrates call sites in `chaoscontrol-explore` and `chaoscontrol-evidence`.
- **APIs**: Public JSON field names should remain stable through compatibility adapters unless a later spec admits a breaking change.
- **Dependencies**: No new runtime service dependency is expected.
- **Testing**: Strict OpenSpec validation now; implementation acceptance requires positive and negative Rust fixture tests plus evidence contract checks.

## Out of Scope

- Moving VM execution, KVM interactions, scheduler state, raw logs, checkpoints, or dogfood process orchestration into the shared core.
- Replacing Nickel evidence contracts at review boundaries.
- Claiming replay verdict compatibility proves global deterministic hypervisor behavior.
