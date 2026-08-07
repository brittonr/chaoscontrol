## Why

`chaoscontrol-vmm` mixes two authorities in one crate. The deterministic simulation state machine (tick loop, vCPU scheduling, network fabric, fault schedule selection, virtual clock, snapshot state model) lives in the same modules as KVM execution (`vm.rs`, `cpu.rs`, `memory.rs`, `devices/`), and `controller.rs` imports `kvm_bindings`/`kvm_ioctls` directly. Three consequences follow:

- The deterministic core cannot be tested, verified, or reused without a KVM-capable host.
- The in-process simulator in `chaoscontrol-evidence` re-implements scheduling, clock, network, and fault concepts because it cannot consume the VMM's simulation logic.
- Downstream consumers (SDK tests, future runtimes) have no way to run deterministic ChaosControl schedules against non-KVM executors.

The replay evidence core extraction (`cairn/archive` / openspec `extract-replay-evidence-core`) established the pattern: a pure Rust-owned core crate with shell adapters. This change applies the same pattern to the simulation engine itself.

## What Changes

- Introduce a `chaoscontrol-sim-core` crate owning pure deterministic simulation logic: tick advancement, scheduling decisions, network fabric state transitions, fault schedule selection, virtual clock, deterministic RNG streams, the event/trace model, and the pure snapshot state model.
- Define a narrow execution boundary: the core emits execution commands and consumes exit observations. All KVM ioctls, guest memory writes, register access, and device MMIO handling move behind shell adapters in `chaoscontrol-vmm`.
- Rewire `SimulationController` so every deterministic decision comes from the core and every machine effect goes through the shell boundary.
- Prove migration equivalence with seeded determinism tests: identical seed, config, and guest artifacts produce identical event traces before and after the split.
- Keep CLI behavior, artifact JSON formats, evidence DTOs, and Nickel contracts unchanged.

## Capabilities

### New Capabilities
- `deterministic-simulation-kernel`: Pure deterministic simulation core and its execution boundary.

## Impact

- **Files**: new `crates/chaoscontrol-sim-core/`; `crates/chaoscontrol-vmm/src/controller.rs`, `scheduler.rs`, network fabric, and device modules split along the core/shell boundary; `chaoscontrol-explore` adopts the core through adapters.
- **APIs**: no public CLI or artifact schema change. Internal module paths change.
- **Dependencies**: the core crate depends only on `serde`, RNG crates, and `chaoscontrol-protocol`; it must not depend on `kvm_bindings`, `kvm_ioctls`, or `linux_loader`.
- **Testing**: core unit tests, purity checks, seeded trace-equivalence tests, existing workspace regression suites, KVM smoke gates.

## Out of Scope

- Changing snapshot completeness semantics or restore preflight rules (owned by the active `complete-vm-snapshot-state` change; this change relocates models only, without semantic edits).
- Unifying the evidence-crate in-process simulator with the extracted core (a later change may converge them).
- Moving the `verified/` Verus function set (may migrate into the core later, unchanged).
- Any hosted, cross-machine, or non-Rust executor claim.
