## Decision

Split the kernel builder into `mkKernel` (returns the kernel derivation) and
`mkChaosKernel` (copies `vmlinux`). `mkKernel { hardening = true; }` merges
`hardenedKernelConfig` after the virtio/KCOV fragments and before caller extras,
so callers can still override. The profile is wrapped in `lib.mkForce` to win over
the common config.

Profile contents:

| Kind | Options |
|---|---|
| Additive hardening | `INIT_ON_ALLOC_DEFAULT_ON`, `SLAB_FREELIST_RANDOM`, `SLAB_FREELIST_HARDENED`, `SHUFFLE_PAGE_ALLOCATOR`, `HARDENED_USERCOPY`, `STRICT_KERNEL_RWX`, `VMAP_STACK`, `RANDOMIZE_BASE`, `STACKPROTECTOR_STRONG` |
| Safe removals | `MAGIC_SYSRQ`, `LEGACY_PTYS`, `DEVPORT` |
| x86_64 additive | `MITIGATION_PAGE_TABLE_ISOLATION`, `MITIGATION_RETPOLINE`, `RANDOMIZE_MEMORY` |

Deliberately excluded: `MODULES`, `DEBUG_FS`, `KALLSYMS`, `IO_URING`,
`USERFAULTFD`, `BPF_SYSCALL`, `COMPAT`/`IA32_EMULATION` and legacy entry points.
Config generation fails or silently keeps them enabled because nixpkgs
common-config selects/requires them (observed: `option not set correctly:
DEBUG_FS wanted 'n', got 'y'`; `repeated question` loops with `MODULES=n`). A
correct removal needs an `allnoconfig` base with an explicit required-symbols
list, which is a separate change.

Exposures:

- `packages.hardened-vmlinux = mkChaosKernel { hardening = true; }`
- `packages.hardened-kernel-config = (mkKernel { hardening = true; }).configfile`
- `checks.hardened-kernel-config` asserts pinned values, including
  `CONFIG_VIRTIO_BLK=y` and `CONFIG_EXT4_FS=y`.

No profile, matrix row, or default package selects the hardened kernel. KCOV
(`kcov = true`) remains the debugging kernel; `hardening` and `kcov` are not
exposed together.

## Verification

- `nix build .#hardened-kernel-config` succeeded; the rendered `.config` shows
  `# CONFIG_MAGIC_SYSRQ is not set`, `# CONFIG_LEGACY_PTYS is not set`,
  `# CONFIG_DEVPORT is not set`, `CONFIG_INIT_ON_ALLOC_DEFAULT_ON=y`,
  `CONFIG_HARDENED_USERCOPY=y`, `CONFIG_STRICT_KERNEL_RWX=y`,
  `CONFIG_VMAP_STACK=y`, `CONFIG_RANDOMIZE_BASE=y`,
  `CONFIG_STACKPROTECTOR_STRONG=y`, with `CONFIG_MODULES=y` and
  `CONFIG_DEBUG_FS=y` retained by design.
- Boot smoke: the hardened `vmlinux` boots the rust-workload initrd on the admitted
  host. The serial log reaches `Run /init as init process`, init reports KCOV is
  absent (expected for this profile), and the VM exits normally after 118,927 exits
  at virtual TSC 13,472,902,427. Artifacts: vmlinux sha256
  `753efb21f1c337e6232c7e8de421947e36662159a4a54fcba95993f5474a7a80`, initrd
  sha256 `385e16afa162712e0d42a5171145f754c00a0bb67382653070baebb62625fea9`.
  This is bounded smoke evidence, not a matrix row or release claim.

## Boundaries

The profile reduces some kernel surface; it does not sandbox a guest or make the
VMM a security boundary. `nix flake check` verifies config values, not runtime
behavior. Boot smoke proves only that this kernel/initrd pair booted under the
recorded bounds.
