# Guest flush model — 2026-10-27

## Scope

This phase implements the virtio flush boundary and the deterministic flush,
cache, and failure semantics. It does not yet implement per-node persistent Raft
disks, protocol storage-recovery oracles, participant-scoped recovery progress,
campaign receipts, or KVM campaigns; those tasks remain open.

Implemented surfaces:

- `crates/chaoscontrol-vmm/src/devices/virtio_request.rs`: `VIRTIO_BLK_T_FLUSH`,
  `BlockOperation::Flush`, and `BlockDeviceProfile` admission. A flush carries
  only header and status descriptors; a data descriptor, a read-only or short
  status buffer, or a device without flush support is refused before any device
  mutation.
- `crates/chaoscontrol-vmm/src/devices/virtio_block.rs`: `VIRTIO_BLK_F_FLUSH`
  is advertised only when the device supports it; admitted flushes execute
  through the block model and complete one status byte. A refused or failed
  flush completes with `VIRTIO_BLK_S_IOERR`, not a dropped queue entry.
- `crates/chaoscontrol-vmm/src/devices/block.rs`: `flush()` binds one attempt
  to a deterministic outcome (`Durable`, `AcknowledgedVolatile`, or an injected
  immediate/delayed error), tracks failed-clean pages so a later successful
  flush cannot silently publish them, and exposes the three restart profiles
  (`ApplicationRestartRetained`, `CacheEvicted`, `GuestReboot`).
- `crates/chaoscontrol-fault/src/{faults,outcomes}.rs`: `DiskFlushError`,
  `BlockFlushError`, `BlockFlushFailed`, and `BlockFlushMadeVolatile` extend the
  closed fault/effect catalogs; lying-flush acknowledgements report
  `BlockFlushMadeVolatile` under `BlockFsyncLie`.

## Baseline

`evidence/baseline.json`, captured before edits:

- `chaoscontrol-vmm` `devices::`: 87 passed.
- `chaoscontrol-smr`: 14 passed.
- `chaoscontrol-raft-guest`: 109 passed.
- `chaoscontrol-redb-guest`: 9 passed.

## Executed verification

```sh
nix develop -c cargo test -p chaoscontrol-vmm --lib
nix develop -c cargo test -p chaoscontrol-vmm --test virtio_block_paths \
  --test virtio_validation --test virtio_properties
nix develop -c cargo test -p chaoscontrol-smr --lib
nix develop -c cargo test -p chaoscontrol-raft-guest
nix develop -c cargo test -p chaoscontrol-redb-guest
nix develop -c cargo fmt --check
nix develop -c cargo clippy -p chaoscontrol-fault -p chaoscontrol-vmm \
  --all-targets -- -D warnings
```

Results:

- `chaoscontrol-vmm --lib`: 551 passed, 0 failed, 9 ignored.
- `virtio_block_paths`: 12 passed; `virtio_validation`: 12 passed;
  `virtio_properties`: 11 passed.
- `chaoscontrol-smr`: 14 passed.
- `chaoscontrol-raft-guest`: 109 passed.
- `chaoscontrol-redb-guest`: 9 passed.
- `cargo fmt --check` and strict Clippy: clean.

Positive coverage: durable flush after a retained-cache write; one-byte status
completion; feature advertisement; successful flush consuming one armed error;
rewrite releasing a failed-clean page.

Negative coverage: flush with a data descriptor; flush without device support;
read-only and short status descriptors; immediate and delayed error completions;
lying flush retaining volatile pages; failed flush retaining pages across a
later successful flush; stale cache surviving application restart and vanishing
after reboot; snapshot validation rejecting failed-clean pages absent from the
volatile layer.

## Non-claims

These are deterministic model observations for the declared code paths. They do
not prove guest kernel, filesystem, device, Redb, Raft, or Molten behavior;
they do not establish campaign-level storage-recovery safety; and they do not
close the open persistent-node, oracle, progress, receipt, or KVM campaign
tasks.
