## Why

ChaosControl's guest binaries and initrd images are built by ad-hoc shell scripts
inside `nix develop`, then checked into git as binary blobs. Running an exploration
requires manually assembling `--kernel`, `--initrd`, and `--disk-image` CLI flags.
There's no way for a downstream consumer to `inputs.chaoscontrol` in their flake and
define simulation tests declaratively.

Nix already builds the VMM, explorer, and kernels. Extending it to cover the full
pipeline — guest binaries, initrd packing, kernel composition, and test orchestration —
turns ChaosControl into something users can adopt by writing a Nix expression instead
of cobbling together shell scripts and paths.

## What Changes

- **Guest binaries become Nix derivations**: Replace `scripts/build-*.sh` with crane
  cross-compilation targeting `x86_64-unknown-linux-musl`. `nix build .#guest-raft`
  produces a statically-linked binary.
- **Initrd packing becomes a Nix function**: `mkChaosInitrd` takes a guest binary
  derivation and produces a cpio+gzip initrd. No more checked-in `guest/*.gz` blobs.
- **Kernel configs become composable**: `mkChaosKernel` merges feature sets (virtio-net,
  kcov, extra config) into a single kernel derivation instead of maintaining separate
  `netKernel` / `kcovKernel` definitions.
- **Declarative test scenarios**: `mkChaosTest` takes a kernel, guest, VM count, round
  count, and seed, and produces a derivation that runs the explorer and fails if any
  assertion is violated. Usable in `nix flake check`.
- **Downstream flake API**: Export `lib.mkChaosInitrd`, `lib.mkChaosKernel`,
  `lib.mkChaosTest` so other flakes can define simulation tests against their own
  guest binaries.
- **Remove `guest/*.gz` from git**: These become build products.
- **Remove `scripts/build-*.sh`**: Replaced by Nix derivations.

## Capabilities

### New Capabilities
- `nix-guest-packages`: Guest binaries as proper Nix packages via crane musl cross-compilation
- `nix-initrd-builder`: `mkChaosInitrd` function that packs a guest binary into a bootable initrd
- `nix-kernel-composer`: `mkChaosKernel` function with composable feature flags (virtio-net, kcov, etc.)
- `nix-test-runner`: `mkChaosTest` function that defines declarative simulation test derivations
- `nix-downstream-api`: Flake outputs (`lib`, `overlays`) for downstream consumer adoption

### Modified Capabilities

## Impact

- **flake.nix**: Major rewrite — new lib functions, new packages, new checks, guest builds moved into Nix
- **scripts/**: `build-guest.sh`, `build-raft-guest.sh`, `build-net-guest.sh` deleted
- **guest/**: `initrd.gz`, `initrd-sdk.gz`, `initrd-raft.gz`, `initrd-net.gz` removed from git
- **.gitignore**: Add `guest/*.gz` (now build artifacts)
- **CI**: `nix flake check` gains simulation test checks (requires `/dev/kvm` passthrough or separate KVM-enabled runner)
- **README.md**: Updated build/run instructions — `nix build .#guest-raft` replaces `scripts/build-raft-guest.sh`
- **Dependencies**: No new external deps — crane and nixpkgs already in flake inputs
