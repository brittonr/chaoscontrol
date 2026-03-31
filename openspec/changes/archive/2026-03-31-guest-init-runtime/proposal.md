## Why

Every ChaosControl guest binary duplicates ~30 lines of unsafe libc mount
calls (devtmpfs, proc, sysfs, debugfs) plus KCOV initialization. A
downstream user writing their first guest has to cargo-cult this
boilerplate from an example and get the unsafe code right. This is the
main friction point preventing external adoption — the Nix composition
layer (mkChaosTest, mkChaosInitrd, mkChaosKernel) is ready, but writing
the guest binary that goes inside is harder than it should be.

## What Changes

- Add `chaoscontrol_sdk::guest_init()` function that handles all VM
  environment setup: mount devtmpfs, proc, sysfs, debugfs, initialize
  KCOV, call `chaoscontrol_init()`. One call replaces the duplicated
  boilerplate.
- Simplify all three existing guest binaries (chaoscontrol-guest,
  chaoscontrol-raft-guest, chaoscontrol-net-guest) to use `guest_init()`
  instead of inline mount code.
- No changes to mkChaosInitrd, mkChaosKernel, mkChaosTest, or the
  explorer. The guest binary is still `/init` (PID 1) — the function
  just handles the environment setup that every PID 1 guest needs.

## Capabilities

### New Capabilities
- `guest-runtime`: SDK function that initializes the VM guest environment (filesystem mounts, KCOV, transport detection), replacing duplicated unsafe boilerplate across guest binaries.

### Modified Capabilities

## Impact

- `crates/chaoscontrol-sdk/src/` — new `runtime.rs` module with `guest_init()`
- `crates/chaoscontrol-guest/src/main.rs` — remove mount functions, call `guest_init()`
- `crates/chaoscontrol-raft-guest/src/main.rs` — remove mount functions, call `guest_init()`
- `crates/chaoscontrol-net-guest/src/main.rs` — remove mount functions, call `guest_init()`
- SDK public API gains one new function in the prelude
