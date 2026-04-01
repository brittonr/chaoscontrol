## 1. Refactor mkChaosKernel

- [ ] 1.1 Add `virtioBlk ? true` parameter to `mkChaosKernel`
- [ ] 1.2 Split virtio config: `VIRTIO`, `VIRTIO_MMIO` enabled when `virtioBlk || virtioNet`; `VIRTIO_BLK` when `virtioBlk`; `VIRTIO_NET`, `PACKET` when `virtioNet`
- [ ] 1.3 Add `EXT4_FS = yes` to the `virtioBlk` config block (needed for mounting the disk image)
- [ ] 1.4 Verify `nix build .#redb-sim --dry-run` still evaluates

## 2. Verify Fix

- [ ] 2.1 Run short redb exploration and confirm "mounted /dev/vda on /data" appears in serial output (no WARNING)
- [ ] 2.2 Verify existing `nix build .#raft-sim --dry-run` still evaluates
- [ ] 2.3 Verify `nix run .#explore-raft -- --help` still works
