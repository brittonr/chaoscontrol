## ADDED Requirements

### Requirement: Guest binaries are Nix derivations

Each guest crate in the workspace (`chaoscontrol-guest`, `chaoscontrol-raft-guest`,
`chaoscontrol-net-guest`) SHALL be buildable as an independent Nix package via
`nix build .#guest-<name>`. The output derivation SHALL contain the statically-linked
musl binary at `$out/bin/<crate-name>`.

#### Scenario: Build SDK guest
- **WHEN** user runs `nix build .#guest-sdk`
- **THEN** `result/bin/chaoscontrol-guest` exists and is a statically-linked x86_64 ELF

#### Scenario: Build Raft guest
- **WHEN** user runs `nix build .#guest-raft`
- **THEN** `result/bin/chaoscontrol-raft-guest` exists and is a statically-linked x86_64 ELF

#### Scenario: Build net guest
- **WHEN** user runs `nix build .#guest-net`
- **THEN** `result/bin/chaoscontrol-net-guest` exists and is a statically-linked x86_64 ELF

### Requirement: Guest builds use musl cross-compilation

Guest derivations SHALL target `x86_64-unknown-linux-musl` and produce fully
static binaries with no dynamic library dependencies.

#### Scenario: No dynamic deps
- **WHEN** `ldd` is run against any guest binary output
- **THEN** it reports "not a dynamic executable" or "statically linked"

### Requirement: Guest builds are independent of VMM build

Building a guest binary SHALL NOT require building the VMM, trace crate, explore
crate, or replay crate. Only protocol and SDK crate sources are needed.

#### Scenario: Guest-only build
- **WHEN** user runs `nix build .#guest-raft`
- **THEN** the build does not compile `chaoscontrol-vmm`, `chaoscontrol-explore`, `chaoscontrol-replay`, or `chaoscontrol-trace`

### Requirement: Shell scripts removed

The files `scripts/build-guest.sh`, `scripts/build-raft-guest.sh`, and
`scripts/build-net-guest.sh` SHALL be deleted from the repository.

#### Scenario: No build scripts
- **WHEN** the change is complete
- **THEN** no `scripts/build-*-guest.sh` files exist in the repo

### Requirement: Pre-built initrd blobs removed from git

The files `guest/initrd.gz`, `guest/initrd-sdk.gz`, `guest/initrd-raft.gz`, and
`guest/initrd-net.gz` SHALL be removed from version control. `.gitignore` SHALL
include `guest/*.gz`.

#### Scenario: No tracked blobs
- **WHEN** `git ls-files guest/` is run
- **THEN** no `.gz` files are listed
