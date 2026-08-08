## Context

Three guest binaries each contain 20-30 lines of unsafe libc calls to
mount devtmpfs, proc, sysfs, and debugfs. The code is nearly identical
across all three, with minor naming differences (`mount_proc` vs
`mount_procfs`). A downstream user writing a new guest must copy this
boilerplate and get the unsafe code right.

The SDK already has `chaoscontrol_init()` which handles transport
detection and catalog emission, but it assumes filesystems are already
mounted. The missing piece is the filesystem setup that happens before
`chaoscontrol_init()`.

## Goals / Non-Goals

**Goals:**
- One function call replaces all guest VM environment setup
- Remove all duplicated mount code from existing guests
- Downstream guest binaries need zero unsafe code for basic operation

**Non-Goals:**
- Separate init wrapper binary (fork+exec model) — future work for non-Rust guests
- C FFI headers — future work
- Changing how mkChaosInitrd works — the guest binary is still `/init`
- Network interface setup (eth0 bringup) — net guest has its own setup beyond mounts

## Decisions

**New `runtime` module in SDK, not a separate crate.**
The mount logic is ~40 lines. A separate crate would add build complexity
for no benefit. The `runtime` module sits next to the existing `lifecycle`,
`coverage`, and `kcov` modules. Gated behind `full` feature like `kcov`.

Alternative: put it in `lifecycle.rs`. Rejected because lifecycle handles
SDK protocol events (setup_complete, send_event), not OS-level setup.

**`guest_init()` calls `chaoscontrol_init()` internally.**
The user calls one function, not two. `chaoscontrol_init()` remains
public for cases where someone wants transport init without mounts (e.g.,
testing the SDK outside a VM), but the normal path is `guest_init()`.

**Mount errors other than EBUSY are logged to stderr, not fatal.**
The VM serial console captures stderr. A missing mount point is not
necessarily fatal — the guest might not need debugfs. Failing hard would
make the function fragile across different kernel configs.

**KCOV init is best-effort.**
`kcov::init()` already returns bool. `guest_init()` calls it and ignores
the return value. Non-KCOV kernels work fine.

## Risks / Trade-offs

**[Risk] Net guest has setup beyond mounts (eth0 bringup, smoltcp)** →
`guest_init()` only handles common setup. Net guest calls `guest_init()`
then does its own network initialization. The 50-line eth0 bringup stays
in the net guest.

**[Risk] Mount order matters** → debugfs requires sysfs at `/sys`.
`guest_init()` mounts in the correct order: devtmpfs → proc → sysfs →
debugfs. This is documented and tested by the existing guests working.

**[Risk] Future mounts needed** → If new guests need additional mounts
(e.g., tmpfs on /tmp), `guest_init()` can grow. The function is the
single place to add them.
