# Nix Downstream Api Specification

## Purpose

Defines the canonical ChaosControl requirements for nix downstream api.

## Requirements
### Requirement: Flake lib output

The flake SHALL export a `lib` output containing `mkChaosInitrd`,
`mkChaosKernel`, and `mkChaosTest` per supported system.

#### Scenario: Downstream access
- **WHEN** a downstream flake adds `inputs.chaoscontrol` and evaluates `chaoscontrol.lib.x86_64-linux.mkChaosTest`
- **THEN** the function is callable and produces a valid derivation

### Requirement: Downstream test definition

A downstream flake SHALL be able to define a simulation test using only
ChaosControl's lib functions and its own guest binary derivation, without
cloning or modifying the ChaosControl repo.

#### Scenario: External guest test
- **WHEN** a downstream flake defines:
  ```nix
  checks.x86_64-linux.my-test = chaoscontrol.lib.x86_64-linux.mkChaosTest {
    name = "my-system";
    kernel = chaoscontrol.lib.x86_64-linux.mkChaosKernel { virtioNet = true; };
    initrd = chaoscontrol.lib.x86_64-linux.mkChaosInitrd {
      init = self.packages.x86_64-linux.my-guest;
    };
    vms = 3; rounds = 100;
  };
  ```
- **THEN** `nix flake check` on the downstream project runs the simulation test

### Requirement: Pre-built kernel reuse

Downstream flakes SHALL be able to reference ChaosControl's pre-built kernel
packages instead of rebuilding kernels locally.

#### Scenario: Use pre-built net kernel
- **WHEN** a downstream flake uses `chaoscontrol.packages.x86_64-linux.net-vmlinux` as the kernel
- **THEN** no kernel compilation occurs in the downstream build

### Requirement: Explorer binary accessible

The `chaoscontrol-explore` binary SHALL be available as a flake package so
downstream users can run ad-hoc explorations without wrapping in `mkChaosTest`.

#### Scenario: Ad-hoc exploration
- **WHEN** user runs `nix run chaoscontrol#explore -- run --kernel ... --initrd ...`
- **THEN** the explorer runs directly

### Requirement: Convenience apps

The flake SHALL export app entries that wire together kernel + initrd + explorer
for built-in scenarios, so a single `nix run` command starts exploration.

#### Scenario: Run raft exploration
- **WHEN** user runs `nix run .#explore-raft`
- **THEN** the explorer starts with the net kernel, raft initrd, and default parameters
