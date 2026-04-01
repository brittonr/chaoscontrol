## MODIFIED Requirements

### Requirement: Nix packaging

The flake SHALL export `guest-redb`, `initrd-redb`, `redb-disk-image`, `explore-redb`, and `redb-sim` derivations.

#### Scenario: Build guest
- **WHEN** `nix build .#guest-redb` runs
- **THEN** a statically-linked musl binary is produced at `$out/bin/chaoscontrol-redb-guest`

#### Scenario: Build initrd
- **WHEN** `nix build .#initrd-redb` runs
- **THEN** a gzipped cpio initrd is produced containing the redb guest as `/init`

#### Scenario: Build disk image
- **WHEN** `nix build .#redb-disk-image` runs
- **THEN** a 16 MB ext4 image is produced, empty and mountable

#### Scenario: Run exploration
- **WHEN** `nix run .#explore-redb` runs
- **THEN** the explorer launches with a kernel that has `CONFIG_VIRTIO_BLK=y`
- **AND** the redb guest successfully mounts `/dev/vda` on `/data`

#### Scenario: Simulation test
- **WHEN** `nix build .#redb-sim` runs (with KVM available)
- **THEN** the simulation completes with disk I/O going through virtio-blk

#### Scenario: Default kernel includes block device
- **WHEN** `mkChaosKernel { }` is evaluated with no arguments
- **THEN** the resulting kernel has `CONFIG_VIRTIO=y`, `CONFIG_VIRTIO_MMIO=y`, and `CONFIG_VIRTIO_BLK=y` built-in

#### Scenario: Existing net kernel unaffected
- **WHEN** `mkChaosKernel { virtioNet = true; }` is evaluated
- **THEN** the resulting kernel has all virtio configs including `VIRTIO_NET=y` and `PACKET=y`

#### Scenario: Block-only kernel omits network
- **WHEN** `mkChaosKernel { }` is evaluated (default `virtioBlk = true`)
- **THEN** the resulting kernel does NOT have `CONFIG_VIRTIO_NET=y` or `CONFIG_PACKET=y`
