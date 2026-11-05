## Why

Capsem's guest kernel is built from `allnoconfig` with hardening pinned
explicitly. ChaosControl's `mkChaosKernel` starts from the nixpkgs common config
and adds virtio/EXT4/KCOV fragments, so the default guest kernel keeps upstream
module, debug, and tracing surfaces. The Capsem comparison
([`docs/capsem-comparison.md`](../../../docs/capsem-comparison.md)) identified an
opt-in hardened kernel as adoptable; issue
[#7](https://github.com/brittonr/chaoscontrol/issues/7) tracks it.

## What Changes

Add `hardening = true` to the kernel builder and expose
`nix build .#hardened-vmlinux` plus `.#hardened-kernel-config`. The profile is
additive: it enables memory/control-flow hardening and disables a small set of
safe, non-cascading surfaces (MAGIC_SYSRQ, legacy PTYs, port access). It does not
disable modules, BPF, io_uring, debugfs, kallsyms, or the legacy ABI because those
symbols are selected or required by the nixpkgs common config; a real
`allnoconfig` base is a separate change. The default kernel, every existing check,
and every matrix row stay unchanged.

A flake check verifies the hardened config's pinned values, and the kernel is
booted on an admitted host with the existing rust-workload initrd as a bounded
smoke, not a matrix row.

## Consumer and Ownership

Consumer: workloads that want a reduced guest kernel surface while keeping the
existing deterministic VMM. Durable capability: an opt-in kernel profile and its
config check. Adoption: caller selects `.#hardened-vmlinux` in a workload profile;
no profile selects it by default. Maintenance owner: chaoscontrol-vmm and flake
maintainers. Repeatability: config assertions run in `nix flake check`; boot smoke
is recorded with its kernel/initrd hashes and limits.

## Non-Goals

No default-kernel change, no support promotion, no claim that the profile provides
sandboxing or isolation, and no matrix/profile selection by default. Whole-subsystem
removal (modules, BPF, io_uring, debugfs, kallsyms, 32-bit compat) requires an
`allnoconfig` base and remains a follow-up. Existing KCOV/debug kernels are
unchanged.
