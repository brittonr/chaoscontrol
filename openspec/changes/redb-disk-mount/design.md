## Context

`mkChaosKernel` currently bundles all virtio config under one `virtioNet` flag:

```nix
VIRTIO = yes;       # core virtio
VIRTIO_MMIO = yes;  # MMIO transport
VIRTIO_NET = yes;   # network device
VIRTIO_BLK = yes;   # block device
PACKET = yes;       # AF_PACKET for raw sockets
```

The redb guest needs `VIRTIO + VIRTIO_MMIO + VIRTIO_BLK` but not `VIRTIO_NET` or `PACKET`. The base kernel (no flags) lacks all of these, so `/dev/vda` never appears and `mount` fails.

## Goals / Non-Goals

**Goals:**
- redb guest's `/dev/vda` mount works
- Disk fault injection (DiskTornWrite, DiskCorruption, etc.) reaches the guest filesystem
- Existing Raft/net/SDK configurations unaffected

**Non-Goals:**
- Changing Rust code
- Optimizing kernel size

## Decisions

Split the virtio config into `virtioBlk` (default true) and `virtioNet` (default false). The base virtio core (`VIRTIO`, `VIRTIO_MMIO`) is enabled whenever either flag is set. `VIRTIO_BLK` follows `virtioBlk`. `VIRTIO_NET` and `PACKET` follow `virtioNet`.

Default `virtioBlk = true` because every ChaosControl VM has a virtio-blk device (the CoW disk). This means `mkChaosKernel { }` now produces a kernel that can mount `/dev/vda` — correct behavior for the common case.

## Risks / Trade-offs

**[Risk]** Adding `VIRTIO_BLK=y` to the default kernel increases size slightly.
→ Negligible. VIRTIO_BLK is a few KB of code. The kernel is already ~17 MB.

**[Risk]** Changing defaults could affect existing callers.
→ The only callers are in flake.nix and all will gain block support, which is correct. No caller was relying on the absence of VIRTIO_BLK.
