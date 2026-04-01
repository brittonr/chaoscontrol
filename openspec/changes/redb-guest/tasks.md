## 1. Crate Scaffolding

- [x] 1.1 Create `crates/chaoscontrol-redb-guest/` with `Cargo.toml` (deps: chaoscontrol-sdk, redb, linkme, libc, serde_json), `src/lib.rs`, `src/main.rs`
- [x] 1.2 Add `crates/chaoscontrol-redb-guest` to workspace `Cargo.toml` members
- [x] 1.3 Verify `cargo build -p chaoscontrol-redb-guest --target x86_64-unknown-linux-musl` compiles

## 2. Guest Main + Disk Mount

- [x] 2.1 `main.rs`: call `guest_init()`, mount devtmpfs on `/dev`, mount `/dev/vda` as ext4 on `/data` (with retry loop for device readiness)
- [x] 2.2 Open or create redb database at `/data/test.redb` using `Database::create()`
- [x] 2.3 Call `coverage::init()`, `kcov::init()`, `lifecycle::setup_complete()` after database opens
- [x] 2.4 Print startup info to serial (database path, key space size)

## 3. Shadow Oracle

- [x] 3.1 Implement `Oracle` struct wrapping `BTreeMap<u64, Vec<u8>>` with insert/delete/get/range/len/snapshot/restore methods
- [x] 3.2 Snapshot method returns a clone, restore replaces internal state
- [x] 3.3 Unit tests for oracle: insert/get/delete/range/snapshot/restore round-trip

## 4. Operation Loop

- [x] 4.1 Define operation types enum: Insert, Read, Delete, RangeScan, Savepoint, Rollback, Compact
- [x] 4.2 Main loop: `random_choice(7)` selects operation type, dispatch to handler functions
- [x] 4.3 Insert handler: `get_random() % 1000` for key, deterministic value (key + counter encoded), write txn, commit, update oracle
- [x] 4.4 Read handler: pick random key, read txn, compare result to oracle
- [x] 4.5 Delete handler: pick random key from oracle keys (or random if empty), write txn, delete, commit, update oracle
- [x] 4.6 Range scan handler: pick two random keys as bounds, read txn, iterate range, compare to oracle range
- [x] 4.7 Savepoint handler: create persistent savepoint, snapshot oracle
- [x] 4.8 Rollback handler: if savepoint exists, restore to it, restore oracle snapshot
- [x] 4.9 Compact handler: call `db.compact()`, verify data unchanged by reading a sample key
- [x] 4.10 `coverage::record_edge()` after each operation with (op_type, outcome) encoding
- [x] 4.11 `kcov::collect()` periodically (every N iterations)

## 5. Assertions

- [x] 5.1 `cc_assert_always!`: read value matches oracle (in read handler)
- [x] 5.2 `cc_assert_always!`: table.len() matches oracle.len() (periodic check)
- [x] 5.3 `cc_assert_always!`: deleted key returns None (in delete handler)
- [x] 5.4 `cc_assert_always!`: range scan keys/values match oracle range
- [x] 5.5 `cc_assert_always!`: database opens successfully (at startup)
- [x] 5.6 `cc_assert_sometimes!`: at least one commit succeeds (in insert handler)
- [x] 5.7 `cc_assert_sometimes!`: large batch committed (batch insert of 10+ keys)
- [x] 5.8 `cc_assert_reachable!` for each operation type (7 calls)
- [x] 5.9 Post-crash verification: on boot, if database file exists, open and compare all keys to oracle

## 6. Error Handling

- [x] 6.1 Handle `redb::Error` variants: DatabaseError, StorageError, TransactionError, CommitError
- [x] 6.2 On DatabaseError (corruption), attempt `Builder::new().create()` to re-open; assert recovery succeeds
- [x] 6.3 On StorageError (I/O, disk full), log and continue loop (fault injection may cause transient failures)
- [x] 6.4 On CommitError, do NOT update oracle (transaction did not commit)

## 7. Nix Packaging

- [x] 7.1 Add `guest-redb = mkGuestPackage { pname = "chaoscontrol-redb-guest"; }` to flake.nix
- [x] 7.2 Add `initrd-redb = mkChaosInitrd { init = guest-redb; name = "chaoscontrol-initrd-redb"; }` to flake.nix
- [x] 7.3 Add `redb-disk-image` derivation: `dd if=/dev/zero of=$out bs=1M count=16 && mkfs.ext4 -F $out`
- [x] 7.4 Add `explore-redb` wrapper script (like explore-raft but --vms 1, --disk-image)
- [x] 7.5 Add `redb-sim = mkChaosTest { ... }` with 1 VM, disk image, appropriate rounds/ticks
- [x] 7.6 Verify `nix build .#guest-redb` produces a musl static binary

## 8. Smoke Test

- [x] 8.1 Build guest + initrd + disk image + kernel via nix
- [ ] 8.2 Run a short exploration (5 rounds, 4 branches, 500 ticks) and verify it completes
- [ ] 8.3 Verify assertion catalog shows all registered assertions
- [ ] 8.4 Verify coverage edges are non-zero
