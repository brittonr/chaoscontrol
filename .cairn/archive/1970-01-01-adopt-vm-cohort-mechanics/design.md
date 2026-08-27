## Context

VM Cohort owns retained initialized bases, private overlays, full-cohort admission, lifecycle, KVM effect execution, cleanup, and bounded conformance. ChaosControl owns the exact snapshot state and deterministic testing meaning applied to those mechanics.

## Decisions

### Decision: Pin one immutable candidate

Cargo and Nix select `ab123e3673b6dd616b3df5d044026b5e85755149`. No branch, sibling path, or moving fallback is allowed.

### Decision: Keep snapshot meaning in ChaosControl

The adapter materializes the reviewed snapshot memory and block effective state into immutable base bytes. It derives VM Cohort compatibility identities from exact ChaosControl cohort facts. VM Cohort never parses ChaosControl fault or replay state.

### Decision: Restore through an application-owned adapter

VM Cohort creates private mappings, VM descriptors, in-kernel devices, and vCPUs. ChaosControl applies `VmSnapshot::restore_devices_only` to the exact clone descriptors before endpoint binding and activation.

### Decision: Compare normalized behavior, not internal roots

The parity corpus compares accepted reads, writes, snapshots, restores, divergence, dirty-page counts, and error classes. Different internal identities or representations do not fail parity when bounded behavior agrees.

### Decision: Preserve authority boundaries

VM Cohort observations are mechanism facts only. ChaosControl continues to plan and attribute faults, choose schedules, evaluate assertions, collect coverage, run exploration and replay, and emit product evidence.

### Decision: Select shared mechanics after parity

The shared path becomes the supported cohort mechanism only after positive, negative, cleanup, and KVM cases pass. Existing duplicate code remains only as explicit diagnostic rollback code until a later removal change.

## Failure behavior

Dependency drift, profile drift, malformed snapshot, restore failure, partial clone, crossed observation, cleanup uncertainty, parity mismatch, or consumer-type leakage fails closed. Unknown outcomes never become success.

## Non-claims

Parity does not prove either implementation correct. KVM smoke does not prove guest correctness, sandboxing, determinism, portability, cleanup erasure, or release eligibility.
