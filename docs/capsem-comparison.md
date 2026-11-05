# Capsem comparison: OCI in a hardened Linux VM

Comparison for issue [#4](https://github.com/brittonr/chaoscontrol/issues/4).
Capsem is Google's Apache-2.0 Rust hypervisor that runs AI agent workloads as OCI
images inside one air-gapped Linux VM per session. This document compares its
device model, guest kernel hardening, container isolation, and determinism
posture with ChaosControl, and records which ideas are adoptable.

Pinned upstream: [`google/capsem` @ `c1018eacbcfb1de12aaadb56b1a388d107ef074d`](https://github.com/google/capsem/tree/c1018eacbcfb1de12aaadb56b1a388d107ef074d).
Files below are cited at that commit. Capsem is a sandbox for untrusted coding
agents; ChaosControl is a deterministic simulation harness for distributed
systems. The goals differ, so the comparison is about transferable mechanisms,
not parity or a security verdict for either project.

## Device model

Capsem's KVM backend exposes a small virtio-mmio surface plus serial:

- [`crates/capsem-core/src/hypervisor/kvm/mod.rs`](https://github.com/google/capsem/blob/c1018eacbcfb1de12aaadb56b1a388d107ef074d/crates/capsem-core/src/hypervisor/kvm/mod.rs)
  composes virtio-mmio slots and requires a full in-kernel IRQ chip, creating an
  in-kernel i8254 PIT when available.
- The device modules are `virtio_blk`, `virtio_console`, `virtio_fs`, `virtio_vsock`,
  `virtio_queue`, plus PIO serial
  ([`kvm/` directory](https://github.com/google/capsem/tree/c1018eacbcfb1de12aaadb56b1a388d107ef074d/crates/capsem-core/src/hypervisor/kvm)).
- The KVM backend is explicitly marked under development (`#[allow(dead_code, …)]`
  on the module) and the hypervisor traits are shared with an Apple
  Virtualization.framework backend
  ([`hypervisor/mod.rs`](https://github.com/google/capsem/blob/c1018eacbcfb1de12aaadb56b1a388d107ef074d/crates/capsem-core/src/hypervisor/mod.rs)).

ChaosControl exposes virtio block, net, entropy, console, MMIO transport, PIT, and
serial devices (`crates/chaoscontrol-vmm/src/devices/`) and drives them from a
deterministic scheduler; the device surface is chosen by the workload profile and
each device is exercised by matrix rows.

Transferable: Capsem keeps the device set minimal per session and treats the
in-kernel PIT as optional with an explicit warning rather than a fallback device.
ChaosControl already chooses devices per profile (net only for net workloads);
no device-model change is adopted from this comparison.

## Guest kernel hardening

Capsem builds the guest kernel from `allnoconfig` and pins the hardening in
[`web/docs/src/content/docs/security/kernel-hardening.md`](https://github.com/google/capsem/blob/c1018eacbcfb1de12aaadb56b1a388d107ef074d/web/docs/src/content/docs/security/kernel-hardening.md)
and the image build tests (`build_system/tests/image/test_kernel_defconfig.py`).
Representative decisions:

| Area | Capsem |
|---|---|
| Modules | `MODULES=n`; no loadable kernel extensions |
| Debug/leak surfaces | `DEVMEM=n`, `DEVPORT=n`, `DEBUG_FS=n`, `KALLSYMS=n` |
| High-CVE syscalls | `BPF_SYSCALL=n`, `IO_URING=n`, `USERFAULTFD=n` |
| Legacy ABI | `COMPAT=n`/`IA32_EMULATION=n`, `X86_VSYSCALL_EMULATION=n`, `LEGACY_PTYS=n` |
| Memory | `INIT_ON_ALLOC_DEFAULT_ON=y`, `SLAB_FREELIST_RANDOM=y`, `SLAB_FREELIST_HARDENED=y`, `SHUFFLE_PAGE_ALLOCATOR=y`, `HARDENED_USERCOPY=y` |
| Control flow | `STRICT_KERNEL_RWX=y`, `VMAP_STACK=y`, `X86_KERNEL_IBT=y`, `X86_USER_SHADOW_STACK=y` |
| Address space | `RANDOMIZE_BASE=y`, `RANDOMIZE_MEMORY=y`, `MITIGATION_PAGE_TABLE_ISOLATION=y`, retpoline |
| Syscall filter | `SECCOMP=y`, `SECCOMP_FILTER=y` |

ChaosControl's `mkChaosKernel` starts from `pkgs.linuxPackages_latest` and adds
virtio/EXT4 fragments plus KCOV options and two TSC/ftrace patches; it does not
pin a hardening allowlist.

Adoptable: an opt-in hardened kernel variant. Filed as
[#7](https://github.com/brittonr/chaoscontrol/issues/7). The default kernel stays
unchanged because KCOV/debug features and workload compatibility are part of the
simulation surface.

## Container isolation inside the guest

Capsem runs one OCI image per VM session through a guest launcher:

- [`guest/artifacts/container/launch.py`](https://github.com/google/capsem/blob/c1018eacbcfb1de12aaadb56b1a388d107ef074d/guest/artifacts/container/launch.py)
  states the image blobs come from a host read-only share, never the writable
  workspace, and assembles an image + per-session layer root.
- [`crates/capsem-core/src/container.rs`](https://github.com/google/capsem/blob/c1018eacbcfb1de12aaadb56b1a388d107ef074d/crates/capsem-core/src/container.rs)
  defines the workload id map: container ids 0–65535 map to VM ids 100000–165535,
  so container root is never VM root.
- [`crates/capsem-core/src/container/seccomp.rs`](https://github.com/google/capsem/blob/c1018eacbcfb1de12aaadb56b1a388d107ef074d/crates/capsem-core/src/container/seccomp.rs)
  vendors moby's default seccomp profile unchanged, resolves its conditions on the
  host, adds Capsem denials, and never grants `CAP_SYS_ADMIN` to the workload.
- [`guest/artifacts/container/runtime_userns.py`](https://github.com/google/capsem/blob/c1018eacbcfb1de12aaadb56b1a388d107ef074d/guest/artifacts/container/runtime_userns.py)
  keeps the launcher as the only user-namespace creator.

ChaosControl lowers reviewed OCI layers into a guest process bundle
([`docs/oci-container-intake.md`](oci-container-intake.md)); it deliberately has no
container runtime, image puller, namespaces, or seccomp policy in the bundle, and
the non-goal stands. The lowered processes inherit the guest's syscall surface.

Adoptable: evaluate bounded uid-map/seccomp admission as an option on top of the
bundle contract, without importing a runtime. Filed as
[#8](https://github.com/brittonr/chaoscontrol/issues/8). The product-scope decision
comes first because "no container runtime" is an explicit non-goal.

## Determinism impact

Capsem's KVM checkpoint stores parked vCPU state and RAM, plus explicit
`KVM_GET_CLOCK`/PIT state
([`kvm/checkpoint.rs`](https://github.com/google/capsem/blob/c1018eacbcfb1de12aaadb56b1a388d107ef074d/crates/capsem-core/src/hypervisor/kvm/checkpoint.rs));
restore applies that clock and PIT state. The clock is anchored to the host clock
and KVM's timer model, the scheduler is the host's, and the project has no virtual
TSC control, deterministic fault scheduling, replay evidence, or equivalently
comparable runs. Its "replay" tests are agent-session and release-lane fixtures,
not VM reproducibility.

ChaosControl's guest time advances through a virtual TSC per exit
(`DEFAULT_TSC_ADVANCE`/`VirtualTsc::tick`) and a deterministic scheduler; snapshots
and replay verdicts bind the exact schedule and fault policy. Importing Capsem's
host-anchored time or adding a non-deterministic control channel (for example an
unbounded vsock request path) would weaken those guarantees, so neither is
adoptable.

Not affected by this comparison: the progress-based TSC idea in issue
[#6](https://github.com/brittonr/chaoscontrol/issues/6), which keeps the virtual
clock authoritative while changing only how it advances.

## README blockers touched

- **Not a hostile-guest sandbox / not a production service**: not applicable to
  this comparison; it is a goal difference, not a Capsem-derived change.
- **Private/nonportable dependencies**: not applicable; Capsem's source layout does
  not change ChaosControl's acquisition boundaries.
- **Production readiness blocked**: not applicable; a comparison adds no runtime
  evidence.
- Adoptable hardening and isolation ideas are tracked as [#7](https://github.com/brittonr/chaoscontrol/issues/7)
  and [#8](https://github.com/brittonr/chaoscontrol/issues/8).

## Non-claims

This comparison does not prove Capsem's isolation, ChaosControl's isolation, or any
shared property. Capsem's performance numbers are its own presentation claims and
were not measured here. No code, kernel configuration, or runtime behavior changed
in ChaosControl as part of this document.
