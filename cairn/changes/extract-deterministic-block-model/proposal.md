## Why

ChaosControl's block backend provides an immutable base image, copy-on-write pages, snapshots, and deterministic storage faults. The useful storage mechanism is mixed with disk-image file loading, VMM configuration, virtio request handling, and ChaosControl fault variants.

A product-neutral crate can support simulator and storage tests without importing KVM or evidence policy. It belongs as an independent package in the planned deterministic simulation repository.

## What Changes

- Add a `deterministic-block` AGPL crate to the shared `deterministic-sim` repository.
- Define caller-owned block geometry, capacity, page size, and transfer limits.
- Split pure range, read, write, flush, fault, and snapshot planning from a bounded in-memory shell.
- Preserve immutable base, durable overlay, volatile overlay, and copy-on-write snapshot semantics.
- Accept explicit fault plans and deterministic tear or corruption parameters instead of selecting randomness internally.
- Keep image-file access, virtio transport, guest memory, VMM fault policy, artifact storage, and evidence in ChaosControl.
- Migrate the ChaosControl block backend only after byte and state-transition parity checks pass.

## Impact

- **Source candidate**: `crates/chaoscontrol-vmm/src/devices/block.rs` and its snapshot and fault adapters.
- **Shared destination**: independent `deterministic-block` crate in `deterministic-sim`.
- **Prerequisites**: shared simulation publication, complete VM snapshots, and explicit fault outcomes.
- **Compatibility**: valid reads, writes, flushes, faults, snapshots, and restore continuation must retain exact behavior.
- **Claims**: the crate models supplied storage transitions. It does not prove durability on a real filesystem or correctness of a guest storage stack.
