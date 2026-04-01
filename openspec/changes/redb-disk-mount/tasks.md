## 1. Refactor mkChaosKernel

- [x] 1.1 Add `virtioBlk ? true` parameter to `mkChaosKernel`
- [x] 1.2 Split virtio config: `VIRTIO`, `VIRTIO_MMIO` enabled when `virtioBlk || virtioNet`; `VIRTIO_BLK` when `virtioBlk`; `VIRTIO_NET`, `PACKET` when `virtioNet`
- [x] 1.3 Add `EXT4_FS = yes` to the `virtioBlk` config block (needed for mounting the disk image)
- [x] 1.4 Verify `nix build .#redb-sim --dry-run` still evaluates

## 2. Verify Fix

- [x] 2.1 Run short redb exploration and confirm "mounted /dev/vda on /data" appears in serial output (no WARNING)
- [x] 2.2 Verify existing `nix build .#raft-sim --dry-run` still evaluates
- [x] 2.3 Verify `nix run .#explore-raft -- --help` still works
