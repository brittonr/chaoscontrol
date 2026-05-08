# Guest Runtime Specification

## Purpose

Defines the canonical ChaosControl requirements for guest runtime.

## Requirements
### Requirement: guest_init function

This requirement MUST be satisfied by the corresponding ChaosControl implementation and validation evidence.

The SDK SHALL export a `guest_init()` function in the `full` feature gate
that performs all VM guest environment setup required before a guest
binary can use the SDK. This function SHALL be safe to call as PID 1 in a
ChaosControl VM.

The function SHALL perform the following steps in order:
1. Mount devtmpfs on `/dev` (ignore EBUSY)
2. Mount proc on `/proc`
3. Mount sysfs on `/sys`
4. Mount debugfs on `/sys/kernel/debug`
5. Initialize KCOV (best-effort, no failure on non-KCOV kernels)
6. Call `chaoscontrol_init()` (transport detection + catalog emission)

#### Scenario: Normal VM boot
- **WHEN** a guest binary calls `guest_init()` as PID 1 in a ChaosControl VM
- **THEN** `/dev`, `/proc`, `/sys`, and `/sys/kernel/debug` are mounted and the SDK transport is initialized

#### Scenario: Already-mounted filesystems
- **WHEN** `guest_init()` is called and devtmpfs is already mounted on `/dev`
- **THEN** the mount returns EBUSY, `guest_init()` ignores the error, and continues with remaining setup

#### Scenario: Non-KCOV kernel
- **WHEN** `guest_init()` runs on a kernel without KCOV support
- **THEN** KCOV initialization fails silently and the function completes normally

### Requirement: guest_init in prelude

This requirement MUST be satisfied by the corresponding ChaosControl implementation and validation evidence.

`guest_init` SHALL be re-exported from `chaoscontrol_sdk::prelude` so
downstream guests can call it with `use chaoscontrol_sdk::prelude::*`.

#### Scenario: Prelude import
- **WHEN** a guest binary uses `use chaoscontrol_sdk::prelude::*`
- **THEN** `guest_init` is available without additional imports

### Requirement: Feature gating

This requirement MUST be satisfied by the corresponding ChaosControl implementation and validation evidence.

`guest_init()` SHALL only be available when the `full` feature is enabled.
Building with `default-features = false` SHALL NOT include the runtime
module.

#### Scenario: No-std build
- **WHEN** the SDK is compiled with `default-features = false`
- **THEN** the `runtime` module and `guest_init` function do not exist

### Requirement: Existing guests use guest_init

This requirement MUST be satisfied by the corresponding ChaosControl implementation and validation evidence.

All existing guest binaries (chaoscontrol-guest, chaoscontrol-raft-guest,
chaoscontrol-net-guest) SHALL be refactored to call `guest_init()` instead
of inline mount functions. No guest binary SHALL contain direct
`libc::mount` calls after this change.

#### Scenario: Raft guest simplified
- **WHEN** `chaoscontrol-raft-guest/src/main.rs` is inspected
- **THEN** `mount_devtmpfs`, `mount_proc`, and equivalent functions are removed, replaced by a single `guest_init()` call

#### Scenario: Net guest simplified
- **WHEN** `chaoscontrol-net-guest/src/main.rs` is inspected
- **THEN** `mount_devtmpfs`, `mount_procfs`, `mount_sysfs`, `mount_debugfs` and equivalent functions are removed, replaced by a single `guest_init()` call

#### Scenario: SDK guest simplified
- **WHEN** `chaoscontrol-guest/src/main.rs` is inspected
- **THEN** `mount_devtmpfs` and equivalent functions are removed, replaced by a single `guest_init()` call
