## Why

The largest VMM and evidence files combine several ownership domains. Their size makes review, unsafe auditing, negative testing, and functional-core extraction harder.

## What Changes

- Define explicit module ownership for VM construction, execution, snapshots, devices, faults, and poison handling.
- Split controller planning from KVM and device effects.
- Split evidence DTO loading, classification, orchestration, and rendering.
- Preserve public Rust APIs, JSON fields, execution semantics, and evidence classes during migration.
- Add dependency-direction and shell-thickness validation.

## Impact

- **Code**: `chaoscontrol-vmm`, `chaoscontrol-evidence`, and shared replay evidence modules.
- **Architecture**: smaller pure cores with thin effect shells.
- **Testing**: baseline parity, API compatibility, negative transitions, and unsafe ownership tests.

## Non-Goals

- No new VMM feature or evidence class.
- No public schema change.
- No claim that smaller files prove safety or correctness.
