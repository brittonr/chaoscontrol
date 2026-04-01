## Why

The only real workload target is Raft (network-heavy, multi-node consensus). redb is a pure-Rust ACID embedded database that exercises a completely different fault surface: disk I/O, crash recovery, B-tree structural integrity, and transaction durability. Adding it as a second guest validates that the SDK, VMM, and exploration engine generalize beyond distributed consensus — and puts the existing disk fault injection (DiskTornWrite, DiskCorruption, DiskSlow, DiskFull) under real load for the first time.

## What Changes

- New `chaoscontrol-redb-guest` crate: single-VM guest that opens a redb database on the virtio-blk device, runs randomized key-value operations (insert, read, delete, range scan, savepoint/rollback), and asserts ACID properties after every transaction
- New Nix packages: `guest-redb`, `initrd-redb`, `explore-redb`, `redb-sim`
- New ext4 disk image for the redb data file (formatted at build time, mounted by the guest)
- SDK assertions covering crash consistency, data integrity, B-tree invariants, and transaction isolation

## Capabilities

### New Capabilities
- `redb-guest`: Guest binary, workload generation, assertion catalog, Nix integration, and disk image creation

### Modified Capabilities

None. The existing SDK, VMM, virtio-blk, fault engine, and exploration infrastructure are used as-is.

## Impact

- New crate: `crates/chaoscontrol-redb-guest/`
- New workspace member in `Cargo.toml`
- New Nix derivations in `flake.nix` (guest-redb, initrd-redb, explore-redb, redb-sim, redb-disk-image)
- redb added as a dependency (pure Rust, no system deps, musl-compatible)
- No changes to existing crates
