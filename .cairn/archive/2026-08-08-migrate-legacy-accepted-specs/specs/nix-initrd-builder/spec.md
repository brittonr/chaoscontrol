# Nix Initrd Builder Specification

## Purpose

Defines the canonical ChaosControl requirements for nix initrd builder.

## Requirements
### Requirement: mkChaosInitrd function

The flake SHALL export a `mkChaosInitrd` function that takes a guest binary
derivation and produces a gzipped cpio initrd image suitable for the ChaosControl
VMM's `--initrd` flag.

#### Scenario: Pack a guest binary
- **WHEN** `mkChaosInitrd { init = guest-raft; }` is evaluated
- **THEN** the output is a single file (not a directory) containing a gzip-compressed cpio archive

#### Scenario: Initrd contains /init
- **WHEN** the initrd is unpacked
- **THEN** it contains an executable `/init` that is the guest binary

### Requirement: Required filesystem directories

The initrd SHALL contain empty directories needed by the guest runtime:
`/dev`, `/proc`, `/sys`, and `/sys/kernel/debug`.

#### Scenario: Default directories
- **WHEN** `mkChaosInitrd { init = guest-sdk; }` is evaluated and unpacked
- **THEN** directories `/dev`, `/proc`, `/sys`, `/sys/kernel/debug` exist in the archive

#### Scenario: Custom extra directories
- **WHEN** `mkChaosInitrd { init = guest-sdk; extraDirs = [ "tmp" "var/log" ]; }` is evaluated
- **THEN** the archive also contains `/tmp` and `/var/log`

### Requirement: Flake initrd packages

The flake SHALL export pre-built initrd packages for each built-in guest:
`initrd-sdk`, `initrd-raft`, `initrd-net`.

#### Scenario: Build raft initrd
- **WHEN** user runs `nix build .#initrd-raft`
- **THEN** the output is a bootable initrd containing the raft guest as `/init`

#### Scenario: Initrd boots in VM
- **WHEN** the initrd from `nix build .#initrd-raft` is passed to `chaoscontrol-explore --initrd`
- **THEN** the VM boots and the guest reaches `setup_complete`
