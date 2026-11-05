## Why

Issue #8 asks whether lowered OCI bundle processes should receive an explicit uid/gid
map and a workload-derived seccomp allowlist without adding a container runtime. The
first step is a pure admission core with typed failures, independent of namespace or
filter installation.

## What Changes

Add `chaoscontrol-protocol::bundle_sandbox`, a bounded admission model for explicit
uid and gid maps and a syscall rule list. Admission rejects empty or oversized maps,
zero-length or overflowing ranges, overlapping inside/outside ranges, guest-root
mapping, missing container-zero mapping, invalid or duplicate syscall names, and
unsupported actions. It installs nothing and proves no isolation; KVM proof and
replay interaction remain separate.

## Impact

- **Files**: `crates/chaoscontrol-protocol/src/bundle_sandbox.rs`,
  `crates/chaoscontrol-protocol/src/lib.rs`
- **Testing**: unit tests cover accepted maps/policies and each rejection class
