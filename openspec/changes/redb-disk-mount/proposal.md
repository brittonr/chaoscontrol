## Why

The redb guest's virtio-blk disk mount fails because `mkChaosKernel { }` (empty config) produces a kernel without `CONFIG_VIRTIO`, `CONFIG_VIRTIO_MMIO`, or `CONFIG_VIRTIO_BLK`. These options are gated behind `virtioNet = true`, which only makes sense for the net guest. The redb guest needs the block device but not the network stack. Without the block device, redb runs on tmpfs and disk fault injection has no effect.

## What Changes

- Refactor `mkChaosKernel` to separate `virtioBlk` from `virtioNet` so block device support can be enabled independently
- Update `redb-sim` and `explore-redb` to use a kernel with block device support
- Add `virtioBlk` flag defaulting to `true` (every guest needs the block device for the CoW disk)

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `redb-guest`: kernel config fix so disk mount succeeds and disk fault injection works

## Impact

- `flake.nix`: `mkChaosKernel` gains `virtioBlk` parameter; existing callers unchanged (default true)
- No Rust code changes
- All existing derivations continue to work (additive kernel config)
