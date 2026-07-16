# Kernel-bundle VM compatibility rail

The kernel-bundle rail executes one exact admitted Onix/Mantle kernel, module,
and BPF cohort inside a disposable ChaosControl KVM guest. It never loads the
artifacts into the host kernel.

## Supported cohort

The first supported cohort is deliberately narrow:

- architecture: `x86_64`;
- kernel release: `6.18.20`;
- kernel input: same-build uncompressed ELF `vmlinux`;
- module: Mantle `private_kfunc.mod.ko`;
- BPF object: Mantle `private_kfunc.ebpf.o`, section and attach class `xdp`;
- attach target: guest loopback interface `lo`;
- required kfunc: `process_value`;
- guest archive: deterministic uncompressed `newc` cpio;
- VMM: `chaoscontrol-vmm::DeterministicVm` with an explicit memory and VM-exit bound.

Other architectures, boot formats, attach classes, targets, modules, and BPF
objects are unsupported until they have their own admitted profile and fixtures.
Unsupported inputs must be reported as blocked, not converted with ambient host
tools.

## Prerequisites

The operator needs:

1. readable `/dev/kvm`;
2. the exact same-build ELF `vmlinux`;
3. the admitted Mantle artifact directory;
4. a static BusyBox;
5. the pinned bpftool;
6. `kernel-bundle-delete-module` built from this repository; and
7. a newline-delimited, absolute, bounded Nix closure list for bpftool, the
   Mantle loader, and the helper.

The initrd builder rejects missing files, relative closure roots, unsupported
file types, unsafe archive paths, excessive entry counts, and excessive script
or closure-list sizes.

## Build the tools and initrd

```console
nix develop -c cargo build -p chaoscontrol-evidence \
  --bin kernel-bundle-vm-compat-smoke \
  --bin kernel-bundle-delete-module

kernel-bundle-vm-compat-smoke --sample-profile > profile.json

kernel-bundle-vm-compat-smoke \
  --build-private-kfunc-initrd private-kfunc-initrd.cpio \
  --artifacts-dir "$MANTLE_PRIVATE_KFUNC_ARTIFACTS" \
  --busybox "$BUSYBOX_STATIC" \
  --bpftool "$BPFTOOL" \
  --delete-module-helper "$KERNEL_BUNDLE_DELETE_MODULE" \
  --closure-list closure-roots.txt \
  > initrd-summary.json
```

Hash the exact kernel and generated initrd with BLAKE3. Both expected values are
mandatory inputs to exact KVM execution; a mismatch is blocked before VMM
creation.

## Run the positive case

```console
kernel-bundle-vm-compat-smoke \
  --kvm-run-profile profile.json \
  --kernel "$VMLINUX" \
  --initrd private-kfunc-initrd.cpio \
  --expected-kernel-blake3 "$VMLINUX_BLAKE3" \
  --expected-initrd-blake3 "$INITRD_BLAKE3" \
  --scenario positive \
  --memory-mib 1024 \
  --max-exits 300000 \
  --out positive-receipt.json
```

A positive receipt passes only when:

- execution mode is `chaoscontrol-vmm-kvm`, never a transcript;
- expected and measured kernel/initrd digests match;
- guest readiness reports the exact kernel release;
- module load, unload, and cleanup observations match;
- BPF verification, attach, detach, and cleanup observations match; and
- the receipt has no issues.

## Negative cases

The same initrd supports deterministic guest scenarios selected through the
VMM-owned kernel command line:

- `missing-kfunc`: omit the private-kfunc module and require the exact object to
  be rejected;
- `verifier-rejection`: require bpftool to reject a fixed malformed BPF object;
- `wrong-attach-target`: require an XDP attach to a nonexistent guest interface
  to fail;
- `cleanup-failure`: require cleanup of the bpffs mount root to fail after the
  positive attach/detach path.

Run each by replacing `--scenario positive` with the scenario name. These
receipts intentionally have `status = failed`; acceptance of the fixture is
recorded separately as `negative_fixture_matched = true` with the exact
`failure_class`.

The `stale-digest` scenario is intentionally different. Supply a stale expected
kernel or initrd digest. The rail records the expected and measured values,
sets `loader_available = false`, and reports `status = blocked` before creating
the VM. A stale input must never become a guest behavior test.

Missing kernel/initrd files and unavailable KVM similarly produce blocked
receipts. A structured marker transcript is useful for parser testing but now
produces `execution-mode-not-exact-kvm`; it cannot pass behavior smoke.

## Guest loader and cleanup

The repo-owned `/init` mounts only the required proc, sysfs, devtmpfs, and bpffs
filesystems; raises `lo`; checks `uname -r`; loads the module; invokes pinned
bpftool and the admitted Mantle loader; and emits bounded structured markers to
COM1. The repo-owned `kernel-bundle-delete-module` helper calls
`delete_module(2)` so dotted generated module names can be removed exactly.

Every run occurs in a fresh disposable VM. Failed cleanup is retained as an
explicit terminal observation and cannot contaminate a later case.

## Evidence and retention

Reviewable receipts live with the active/archived Cairn change evidence. Keep:

- the profile and profile identity;
- initrd summary and measured digest;
- exact positive receipt;
- stale-input blocked receipt;
- all selected negative receipts;
- focused test, clippy, and Cairn gate transcripts.

The approximately 200 MiB initrd, raw serial output, kernel image, and unbounded
verifier logs are local debug/build artifacts and are not committed evidence.
The receipt includes no host paths or artifact bytes.

## Non-claims

A passing receipt proves only bounded compatibility smoke for one exact
kernel/initrd/profile/artifact/VMM cohort. It is not proof of universal
bootability, module or eBPF safety, kernel correctness, build correctness,
ChaosControl deterministic replay, Onix lifecycle replay, physical readiness,
security, deployability, or release eligibility. The pure evidence-role guard
rejects all of those promotions.
