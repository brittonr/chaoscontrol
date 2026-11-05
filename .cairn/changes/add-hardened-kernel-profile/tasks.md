## Tasks

- [x] [serial] Split the kernel builder and add the opt-in additive hardening profile. r[chaoscontrol.hardened_kernel.profile]
- [x] [serial] Expose `hardened-vmlinux` and `hardened-kernel-config` and add the config-value check. r[chaoscontrol.hardened_kernel.profile]
- [x] [serial] Verify the config check; retain the boot-smoke evidence and limits. r[chaoscontrol.hardened_kernel.profile]
- [x] [serial] Build an `allnoconfig` base with an explicit required-symbols list for module/debugfs/kallsyms/BPF/io_uring/legacy-ABI removals. r[chaoscontrol.hardened_kernel.allnoconfig_gate]
- [ ] [serial] Add a matrix/profile selection decision and workload compatibility evidence before any default use. r[chaoscontrol.hardened_kernel.profile]

## Evidence

`nix build .#hardened-kernel-config` succeeded and `checks.hardened-kernel-config`
passes. The allnoconfig variant (`hardenedMinimalKernelConfig`,
`checks.x86_64-linux.hardened-minimal-kernel-config`) removes modules, debugfs,
kallsyms, BPF, io_uring, userfaultfd, and 32-bit emulation and keeps
`X86_IOPL_IOPERM` for the port-I/O hypercall transport; its ~14 MB vmlinux
booted to `Run /init` but the workload did not exit within 90 s, so it is not
admitted for workloads. `nix build .#hardened-vmlinux` produced vmlinux sha256
`753efb21…a7a80`; boot smoke with initrd sha256 `385e16af…5fea9` reached
`Run /init as init process` (init reported KCOV absent, as expected) and the VM
exited normally after 118,927 exits at virtual TSC 13,472,902,427. The
default kernel and all existing checks are unchanged. Overriding MODULES/DEBUG_FS/
KALLSYMS/IO_URING on the common config failed config generation or silently stayed
enabled; those removals are gated behind `allnoconfig`. No sandbox, isolation,
correctness, or support-promotion claim.
