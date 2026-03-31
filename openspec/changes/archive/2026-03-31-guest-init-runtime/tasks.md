## 1. SDK runtime module

- [x] 1.1 Create `crates/chaoscontrol-sdk/src/runtime.rs` with `guest_init()` function gated behind `#[cfg(feature = "full")]`. Implementation: mkdir+mount devtmpfs `/dev` (ignore EBUSY), mkdir+mount proc `/proc`, mkdir+mount sysfs `/sys`, mkdir+mount debugfs `/sys/kernel/debug`, call `kcov::init()` (ignore return), call `chaoscontrol_init()`, call `coverage::init()`
- [x] 1.2 Wire `runtime` module into `lib.rs`: `#[cfg(feature = "full")] pub mod runtime;`
- [x] 1.3 Re-export `guest_init` from `prelude.rs`: `pub use crate::runtime::guest_init;` (feature-gated)

## 2. Migrate existing guests

- [x] 2.1 Simplify `chaoscontrol-guest/src/main.rs`: remove `mount_devtmpfs()` function, replace the init sequence (`mount_devtmpfs(); chaoscontrol_init(); coverage::init(); kcov::init();`) with a single `guest_init()` call
- [x] 2.2 Simplify `chaoscontrol-raft-guest/src/main.rs`: remove `mount_devtmpfs()` and `mount_proc()` functions, replace the init sequence (`mount_devtmpfs(); mount_proc(); chaoscontrol_init(); coverage::init(); kcov::init();`) with a single `guest_init()` call. Keep `parse_bug_mode()` which reads `/proc/cmdline` (proc is now mounted by `guest_init()`)
- [x] 2.3 Simplify `chaoscontrol-net-guest/src/main.rs`: remove `mount_devtmpfs()` and `mount_procfs()` functions, replace `chaoscontrol_sdk::chaoscontrol_init(); mount_devtmpfs(); mount_procfs(); coverage::init();` with a single `guest_init()` call

## 3. Verify

- [x] 3.1 `cargo build --workspace` compiles cleanly
- [x] 3.2 `cargo clippy --workspace --all-targets -- --deny warnings` passes
- [x] 3.3 `cargo test --workspace` — all existing tests pass
- [x] 3.4 Grep for `libc::mount` in guest crates — zero hits in chaoscontrol-guest, chaoscontrol-raft-guest, chaoscontrol-net-guest
