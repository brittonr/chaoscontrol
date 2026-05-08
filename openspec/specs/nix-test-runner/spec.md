# Nix Test Runner Specification

## Purpose

Defines the canonical ChaosControl requirements for nix test runner.

## Requirements
### Requirement: mkChaosTest function

The flake SHALL export a `mkChaosTest` function that takes a kernel, initrd,
and exploration parameters, and produces a derivation that runs
`chaoscontrol-explore run` and fails if any `always` assertion is violated.

#### Scenario: Passing test
- **WHEN** `mkChaosTest { name = "trivial"; kernel = ...; initrd = ...; rounds = 5; }` is built and the guest has no assertion failures
- **THEN** the derivation succeeds and `$out/report.txt` contains the exploration report

#### Scenario: Failing test
- **WHEN** `mkChaosTest` is built and the guest has an `always` assertion violation
- **THEN** the derivation fails with a non-zero exit code

### Requirement: KVM system feature

`mkChaosTest` derivations SHALL declare `requiredSystemFeatures = [ "kvm" ]`
so Nix builders without KVM skip them instead of failing.

#### Scenario: Builder without KVM
- **WHEN** `nix flake check` runs on a machine without `kvm` in system-features
- **THEN** KVM-dependent checks are skipped, not errored

### Requirement: Exploration parameters

`mkChaosTest` SHALL accept optional parameters for the exploration run:
`vms` (default 3), `rounds` (default 50), `branches` (default 8),
`ticks` (default 1000), `seed` (default 42), `mode` (default "hybrid"),
`diskImage` (default null), `extraArgs` (default "").

#### Scenario: Custom parameters
- **WHEN** `mkChaosTest { ...; vms = 5; rounds = 200; seed = 1337; mode = "input-tree"; }` is evaluated
- **THEN** the explorer is invoked with `--vms 5 --rounds 200 --seed 1337 --mode input-tree`

### Requirement: Output artifacts

A successful `mkChaosTest` build SHALL produce the standard explorer output
in `$out/`: `report.txt`, `assertions.json`, `checkpoint.json`, and any
`bug_N.json` files.

#### Scenario: Report files present
- **WHEN** a `mkChaosTest` derivation succeeds
- **THEN** `$out/report.txt` and `$out/assertions.json` exist

### Requirement: Pre-built test checks in flake

The flake SHALL include pre-assembled simulation tests as `checks` that run
the built-in guests (raft, sdk) against appropriate kernels.

#### Scenario: Raft safety check
- **WHEN** `nix flake check` runs on a KVM-capable machine
- **THEN** a raft simulation test runs with the net kernel and raft initrd

### Requirement: Tuned explore-redb wrapper
The `explore-redb` nix app SHALL use defaults appropriate for single-node storage testing: `--vms 1`, `--ticks 5000` (long enough for multiple crash/recovery cycles), `--rounds 100`, `--branches 8`, and `--mode hybrid`. The wrapper SHALL pass the `redb-disk-image` via `--disk-image`.

#### Scenario: Explore-redb defaults
- **WHEN** `nix run .#explore-redb` is invoked without arguments
- **THEN** the explorer starts with 1 VM, 5000 ticks per branch, and the redb disk image

#### Scenario: User override via extra args
- **WHEN** `nix run .#explore-redb -- --rounds 500 --seed 99`
- **THEN** the user-specified flags override the wrapper defaults

### Requirement: Adequate redb disk image size
The `redb-disk-image` nix derivation SHALL produce an ext4 filesystem image with at least 64 MB of usable space. This SHALL accommodate the redb guest's workload (MAX_KEY=1000, values up to 64 bytes) across multiple crash/recovery cycles with compaction and WAL growth.

#### Scenario: Image is large enough for workload
- **WHEN** the redb guest runs for 5000 ticks across 100 branches
- **THEN** no `ENOSPC` errors occur unless a `DiskFull` fault was explicitly injected
