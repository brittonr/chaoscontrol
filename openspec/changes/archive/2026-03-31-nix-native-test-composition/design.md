## Context

ChaosControl's build pipeline has a split: the VMM, explorer, and kernels are
Nix derivations, but guest binaries and initrd images are built by imperative
shell scripts that only work inside `nix develop`. The shell scripts all do the
same thing — `cargo build --target x86_64-unknown-linux-musl`, `cpio`, `gzip` —
but each is a separate file with duplicated logic.

The flake currently exports `packages` (the VMM binaries), `checks` (unit tests,
clippy, fmt), and kernel derivations (`net-vmlinux`, `kcov-vmlinux`). It does not
export any library functions for downstream use.

Crane already handles the workspace build. The musl cross-compiler
(`pkgs.pkgsCross.musl64.stdenv.cc`) is in the devShell. `.cargo/config.toml` sets
the musl linker.

## Goals / Non-Goals

**Goals:**
- Guest binaries built by `nix build .#guest-raft` — no shell scripts, no `nix develop`
- Initrd images are build products, not git-tracked blobs
- A downstream flake can `inputs.chaoscontrol` and define simulation tests
- `nix flake check` can include simulation tests (on KVM-capable machines)
- Kernel feature sets composable without duplicating full kernel definitions
- Existing CLI (`chaoscontrol-explore run --kernel ... --initrd ...`) unchanged

**Non-Goals:**
- Docker/OCI support
- Multi-language SDK
- Remote/SaaS test execution
- Changing the explorer's internal architecture
- NixOS module for running ChaosControl as a service

## Decisions

### 1. Separate crane builds for guest crates

**Choice:** Build guest crates (`chaoscontrol-guest`, `chaoscontrol-raft-guest`,
`chaoscontrol-net-guest`) as independent crane derivations targeting
`x86_64-unknown-linux-musl`, separate from the main workspace build.

**Rationale:** Guest binaries cross-compile to musl. The main workspace builds
for the host (glibc, with BPF toolchain deps). Mixing both in one crane
invocation means the entire workspace rebuilds when switching targets. Separate
builds also mean `nix build .#guest-raft` works without building the VMM.

**Alternative:** Build the entire workspace twice (host + musl). Wasteful —
doubles compile time for unrelated crates.

**Implementation:** Use `craneLib.buildPackage` with `CARGO_BUILD_TARGET` set to
`x86_64-unknown-linux-musl` and a separate `cargoArtifacts` for the musl
dependency cache. Filter `src` to include only the guest crate and its workspace
dependencies (protocol, SDK). The musl linker comes from
`pkgs.pkgsCross.musl64.stdenv.cc`.

Crane's `buildPackage` with `cargoExtraArgs = "-p chaoscontrol-raft-guest"` builds
one crate from the workspace. Combined with the musl target, this produces the
static binary without needing to restructure the workspace.

### 2. mkChaosInitrd as a runCommand

**Choice:** `mkChaosInitrd` is a plain Nix function that calls `cpio` and `gzip`
inside a `pkgs.runCommand` derivation.

**Rationale:** The initrd packing is trivial (~5 lines of shell). A `runCommand`
is the simplest Nix primitive that does the job. No need for a custom builder or
`writeShellApplication`.

**Alternative:** Use `pkgs.makeInitrd` (NixOS's initrd builder). Overkill — it
handles kernel modules, udev, systemd, none of which ChaosControl needs. The
ChaosControl initrd is literally one binary at `/init` plus empty mount dirs.

**Implementation:**
```nix
mkChaosInitrd = { init, name ? "chaoscontrol-initrd", extraDirs ? [ "dev" "proc" "sys/kernel/debug" ] }:
  pkgs.runCommand name {
    nativeBuildInputs = [ pkgs.cpio ];
  } ''
    mkdir -p root
    for d in ${lib.concatStringsSep " " extraDirs}; do
      mkdir -p "root/$d"
    done
    cp ${init}/bin/* root/init
    chmod +x root/init
    (cd root && find . -print0 | cpio --null -o -H newc --quiet) | gzip -9 > $out
  '';
```

Output is a single file (the `.gz`), not a directory. This matches what the
explorer CLI expects for `--initrd`.

### 3. mkChaosKernel with structured config merge

**Choice:** `mkChaosKernel` takes a set of boolean flags and an `extraConfig`
attrset, merges them into `structuredExtraConfig`, and calls
`linuxPackages_latest.kernel.override`.

**Rationale:** The current flake has `netKernel` and `kcovKernel` as separate
definitions that can't be combined. A user who wants both virtio-net AND kcov
must duplicate the config. A merge function fixes this.

**Alternative:** Use a NixOS-style module system with options. Overkill for
4-5 boolean flags.

**Implementation:**
```nix
mkChaosKernel = {
  virtioNet ? false,
  kcov ? false,
  extraConfig ? {},
}: let
  cfg = with lib.kernel;
    (lib.optionalAttrs virtioNet {
      VIRTIO = yes; VIRTIO_MMIO = yes; VIRTIO_NET = yes;
      VIRTIO_BLK = yes; PACKET = yes;
    })
    // (lib.optionalAttrs kcov {
      KCOV = yes; KCOV_INSTRUMENT_ALL = yes;
      KCOV_ENABLE_COMPARISONS = yes; DEBUG_FS = yes;
    })
    // extraConfig;
in pkgs.linuxPackages_latest.kernel.override {
  structuredExtraConfig = cfg;
};
```

Callers: `mkChaosKernel { virtioNet = true; kcov = true; }` gives a single
kernel with both feature sets.

### 4. mkChaosTest wraps the explorer CLI

**Choice:** `mkChaosTest` produces a derivation that runs `chaoscontrol-explore run`
with the given parameters and fails if any `always` assertion is violated (non-zero
exit code).

**Rationale:** The explorer already produces structured output (`assertions.json`,
exit codes). Wrapping it in a Nix derivation means `nix flake check` can include
simulation tests — the standard Nix CI pattern.

**Alternative:** Write a custom Nix test framework (like `testers.runNixOSTest`).
Unnecessary — the explorer CLI already does everything. The derivation just needs
to invoke it and check the exit code.

**Implementation:**
```nix
mkChaosTest = {
  name,
  kernel,          # vmlinux derivation
  initrd,          # output of mkChaosInitrd
  vms ? 3,
  rounds ? 50,
  branches ? 8,
  ticks ? 1000,
  seed ? 42,
  mode ? "hybrid",
  diskImage ? null,
  extraArgs ? "",
}: pkgs.runCommand "chaoscontrol-test-${name}" {
  nativeBuildInputs = [ chaoscontrol ];
  requiredSystemFeatures = [ "kvm" ];
} ''
  mkdir -p $out
  chaoscontrol-explore run \
    --kernel ${kernel}/vmlinux \
    --initrd ${initrd} \
    --vms ${toString vms} \
    --rounds ${toString rounds} \
    --branches ${toString branches} \
    --ticks ${toString ticks} \
    --seed ${toString seed} \
    --mode ${mode} \
    ${lib.optionalString (diskImage != null) "--disk-image ${diskImage}"} \
    ${extraArgs} \
    --output $out
'';
```

`requiredSystemFeatures = [ "kvm" ]` tells Nix the derivation needs `/dev/kvm`.
On Hydra or `nix flake check`, the builder must have `system-features = kvm` in
`nix.conf`. Without it, Nix skips the derivation gracefully rather than failing
on a machine without KVM.

### 5. Flake lib exports

**Choice:** Export `lib.mkChaosInitrd`, `lib.mkChaosKernel`, `lib.mkChaosTest`
at the flake level so downstream flakes can call them.

**Rationale:** This is the standard pattern for reusable Nix libraries (crane,
dream2nix, etc.). A downstream user writes:

```nix
{
  inputs.chaoscontrol.url = "github:user/chaoscontrol";
  outputs = { self, chaoscontrol, nixpkgs, ... }:
    let
      cc = chaoscontrol.lib.x86_64-linux;
    in {
      checks.x86_64-linux.raft-test = cc.mkChaosTest {
        name = "raft-safety";
        kernel = cc.mkChaosKernel { virtioNet = true; };
        initrd = cc.mkChaosInitrd {
          init = self.packages.x86_64-linux.my-raft-guest;
        };
        vms = 3;
        rounds = 100;
      };
    };
}
```

**Alternative:** Export an overlay instead. Overlays are less explicit — the user
has to know which attributes the overlay adds. Direct lib functions are clearer.

**Implementation:** The flake output adds:
```nix
lib = forAllSystems (system: {
  mkChaosInitrd = ...;
  mkChaosKernel = ...;
  mkChaosTest = ...;
});
```

### 6. vmlinux extraction stays as-is

**Choice:** Keep the `pkgs.runCommand` that creates `$out/vmlinux` symlink from
the kernel's `.dev` output.

**Rationale:** `mkChaosKernel` returns a kernel derivation. The vmlinux ELF is in
`kernel.dev/vmlinux`. The explorer CLI expects a path to the ELF file. The
`runCommand` wrapper creates a consistent `$out/vmlinux` path. `mkChaosTest`
references `${kernel}/vmlinux` — this works if the kernel output is the
runCommand wrapper, or if we add a `mkChaosVmlinux` helper.

**Implementation:** Either:
- `mkChaosTest` knows to look at `${kernel.dev}/vmlinux` directly
- Or `mkChaosKernel` returns the `runCommand` wrapper (current pattern)

Use the wrapper approach — keeps the public API clean. `mkChaosKernel` returns
something where `$out/vmlinux` is the ELF.

## Risks / Trade-offs

**[Crane musl cross-compilation may need workspace partitioning]** → The guest
crates depend on `chaoscontrol-protocol` and `chaoscontrol-sdk`, which are
`no_std`-compatible. Crane should handle building these as musl deps without
issue. If it doesn't, fall back to `cargoExtraArgs = "--target x86_64-unknown-linux-musl"`
on a workspace-wide build and extract only the guest binary.

**[KVM requirement for mkChaosTest]** → Nix sandbox doesn't expose `/dev/kvm` by
default. `requiredSystemFeatures = [ "kvm" ]` is the standard escape hatch, but
it means simulation tests can't run on pure-sandbox CI (GitHub Actions). Machines
need `nix.conf` with `system-features = kvm`. Mitigation: keep simulation tests
as separate `checks` that CI can skip; unit tests run everywhere.

**[Build time for custom kernels]** → First `mkChaosKernel` build takes ~20 minutes.
Nix caches aggressively, so subsequent builds are instant unless the kernel version
bumps. Downstream users who don't need custom kernels can use the pre-built ones
from ChaosControl's flake outputs.

**[initrd size with debug symbols]** → Crane's release build strips debug info by
default. If a user builds in debug mode, the initrd could be large (10s of MB).
The musl static binary in release mode is typically <1 MB. Not a real problem for
the default path.

**[Removing guest/*.gz breaks anyone cloning and running without nix build]** →
The README currently says "just run `cargo run --bin boot -- result-dev/vmlinux
guest/initrd.gz`". After this change, users must `nix build .#initrd-sdk` first.
Mitigation: update README, add a convenience `nix run .#explore-raft` app that
wires everything together.
