# Nix Kernel Composer Specification

## Purpose

Defines the canonical ChaosControl requirements for nix kernel composer.

## Requirements
### Requirement: mkChaosKernel function

The flake SHALL export a `mkChaosKernel` function that takes boolean feature
flags and an optional `extraConfig` attrset, and returns a kernel derivation
where `$out/vmlinux` is the ELF kernel.

#### Scenario: Default kernel
- **WHEN** `mkChaosKernel {}` is evaluated
- **THEN** the output contains `vmlinux` and the kernel has no extra config beyond the base

#### Scenario: Virtio-net kernel
- **WHEN** `mkChaosKernel { virtioNet = true; }` is evaluated
- **THEN** the kernel is built with `VIRTIO=y`, `VIRTIO_MMIO=y`, `VIRTIO_NET=y`, `VIRTIO_BLK=y`, `PACKET=y`

#### Scenario: KCOV kernel
- **WHEN** `mkChaosKernel { kcov = true; }` is evaluated
- **THEN** the kernel is built with `KCOV=y`, `KCOV_INSTRUMENT_ALL=y`, `KCOV_ENABLE_COMPARISONS=y`, `DEBUG_FS=y`

### Requirement: Feature composition

Multiple feature flags SHALL be composable in a single call. The resulting
kernel has the union of all requested config options.

#### Scenario: Combined virtio-net and kcov
- **WHEN** `mkChaosKernel { virtioNet = true; kcov = true; }` is evaluated
- **THEN** the kernel has both virtio and kcov config options enabled

#### Scenario: Extra config merged
- **WHEN** `mkChaosKernel { virtioNet = true; extraConfig = { PRINTK_TIME = lib.kernel.yes; }; }` is evaluated
- **THEN** the kernel has virtio config plus `PRINTK_TIME=y`

### Requirement: Existing kernel packages use mkChaosKernel

The existing `net-vmlinux` and `kcov-vmlinux` flake packages SHALL be defined
in terms of `mkChaosKernel` instead of inline `kernel.override` calls.

#### Scenario: net-vmlinux
- **WHEN** `nix build .#net-vmlinux` is run
- **THEN** the output is identical to `mkChaosKernel { virtioNet = true; }`

#### Scenario: kcov-vmlinux
- **WHEN** `nix build .#kcov-vmlinux` is run
- **THEN** the output is identical to `mkChaosKernel { kcov = true; }`

### Requirement: vmlinux output path

The output of `mkChaosKernel` SHALL be a derivation where the vmlinux ELF is
at `$out/vmlinux`. This matches the convention used by `mkChaosTest`.

#### Scenario: Consistent path
- **WHEN** any kernel built with `mkChaosKernel` is inspected
- **THEN** `$out/vmlinux` is a valid x86_64 ELF file
