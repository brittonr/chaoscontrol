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

## Persistent nodes, oracles, and progress

`crates/chaoscontrol-smr/src/persistence.rs` adds a bounded, pure node storage
model: `PersistentNodeState` (term, vote, snapshot, retained log suffix, and
application bytes), a versioned length-framed record with a BLAKE3 identity, and
`SmrDisk` stage/flush/load with write failure, flush failure, corruption,
truncation, capacity, and framing denials. `PersistentNodeCohort` allocates one
independent disk per declared participant. The model performs no host I/O.

`crates/chaoscontrol-smr/src/storage_recovery.rs` adds the protocol safety
oracle and participant-scoped progress decision. The oracle reports
acknowledged-data loss, committed-entry truncation, unsafe post-repair voting,
conflicting committed history, and progress that assumes unknown commitment.
Progress is `pass`, `fail`, `not-evaluated`, or `incomplete` and distinguishes
local sufficiency, admitted-peer repair, missed available repair, complete
global absence, unreached horizon, and incomplete observation.

Executed:

```sh
nix develop -c cargo test -p chaoscontrol-smr --lib
nix develop -c cargo fmt --check
nix develop -c cargo clippy -p chaoscontrol-smr --all-targets -- -D warnings
```

Results: 28 passed (14 pre-existing + 6 persistence + 8 recovery), 0 failed;
formatting and strict Clippy clean.

Positive coverage: full member round trip, crash-before-flush retaining only
published state, independent cohort disks, consistent durable state passing the
safety oracle, local sufficiency passing without remote repair, admitted-peer
repair passing, complete global absence as fail-closed unavailability.

Negative coverage: write failure, capacity denial, corruption, truncation,
inconsistent log gap, snapshot/log overlap, truncated/acknowledged loss, unsafe
voting, conflicting committed history, unknown commitment, unnecessary remote
repair, missed available repair, unreached horizon, and incomplete observation.

Still open: workload schedules (restart, lagging replica, unavailable peer,
corruption, local repair), receipt cohort binding, saved-evidence campaigns,
and the closing KVM/Nix gates. These are not claimed by the tests above.

## Non-claims

These are deterministic model observations for the declared code paths. They do
not prove guest kernel, filesystem, device, Redb, Raft, or Molten behavior;
they do not establish campaign-level storage-recovery safety; and they do not
close the open persistent-node, oracle, progress, receipt, or KVM campaign
tasks.
