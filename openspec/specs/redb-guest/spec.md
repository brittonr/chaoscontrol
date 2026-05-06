# redb-guest Specification

## Purpose
TBD - created by archiving change redb-guest. Update Purpose after archive.
## Requirements
### Requirement: Guest binary runs as PID 1

The `chaoscontrol-redb-guest` binary SHALL run as PID 1 inside a ChaosControl VM, mount devtmpfs on `/dev`, mount the virtio-blk device as ext4 on `/data`, open a redb database at `/data/test.redb`, and execute operations in an infinite loop.

#### Scenario: Successful boot and database open
- **WHEN** the VM boots with the redb guest initrd and a formatted ext4 disk image
- **THEN** the guest mounts `/dev/vda` on `/data`
- **AND** the guest opens or creates `/data/test.redb` via `Database::create()` or `Database::open()`
- **AND** the guest prints a startup message to serial

#### Scenario: Guest loops forever
- **WHEN** the guest completes its initialization
- **THEN** the main loop runs indefinitely (no exit or workload-complete pattern)
- **AND** the VMM controls the execution horizon via `run_bounded()`

### Requirement: Randomized operation workload

The guest SHALL use `random_choice()` to select operations each iteration, covering: insert, read, delete, range scan, savepoint, rollback, and compaction. All randomness SHALL flow through the ChaosControl SDK.

#### Scenario: Operation selection
- **WHEN** the guest enters a loop iteration
- **THEN** it calls `random_choice()` to select an operation type
- **AND** it calls `random_choice()` or `get_random()` for operation parameters (key, value size, range bounds)

#### Scenario: Insert operation
- **WHEN** the selected operation is insert
- **THEN** the guest opens a write transaction, inserts a key-value pair, and commits
- **AND** the shadow oracle is updated with the same key-value pair after commit succeeds

#### Scenario: Read operation
- **WHEN** the selected operation is read
- **THEN** the guest opens a read transaction and reads a key
- **AND** the result is compared against the shadow oracle

#### Scenario: Delete operation
- **WHEN** the selected operation is delete
- **THEN** the guest opens a write transaction, deletes a key, and commits
- **AND** the shadow oracle removes the same key after commit succeeds

#### Scenario: Range scan operation
- **WHEN** the selected operation is range scan
- **THEN** the guest opens a read transaction and iterates a key range
- **AND** the returned keys and values match the shadow oracle's range

#### Scenario: Savepoint and rollback
- **WHEN** the selected operation is savepoint
- **THEN** the guest creates a persistent savepoint
- **WHEN** the selected operation is rollback and a savepoint exists
- **THEN** the guest restores to the savepoint and reverts the shadow oracle to its snapshot

#### Scenario: Compaction
- **WHEN** the selected operation is compaction
- **THEN** the guest calls `db.compact()` and verifies data is unchanged afterward

### Requirement: Shadow oracle tracks committed state

The guest SHALL maintain a `BTreeMap<u64, Vec<u8>>` as the ground truth for committed database state. The oracle SHALL be updated only after a transaction successfully commits. Read operations SHALL compare redb results against the oracle.

#### Scenario: Oracle updated on commit
- **WHEN** a write transaction commits successfully
- **THEN** the oracle reflects the insert or delete from that transaction

#### Scenario: Oracle unchanged on abort
- **WHEN** a write transaction fails or is rolled back
- **THEN** the oracle is unchanged

#### Scenario: Oracle used for verification
- **WHEN** a read transaction reads key K
- **THEN** if the oracle contains K, redb SHALL return the same value
- **AND** if the oracle does not contain K, redb SHALL return None

### Requirement: ACID assertion catalog

The guest SHALL register assertions via `cc_assert_always!` and `cc_assert_sometimes!` macros covering data integrity, crash consistency, and liveness.

#### Scenario: Committed data survives
- **WHEN** the guest reads a key that exists in the oracle
- **THEN** `cc_assert_always!` verifies redb returns the matching value

#### Scenario: Database opens after crash
- **WHEN** the guest boots and opens the database file
- **THEN** `cc_assert_always!` verifies `Database::open()` or `Database::create()` succeeds without error

#### Scenario: Table length matches
- **WHEN** the guest queries `table.len()` in a read transaction
- **THEN** `cc_assert_always!` verifies it equals `oracle.len()`

#### Scenario: Delete removes key
- **WHEN** a delete transaction commits and the guest re-reads the key
- **THEN** `cc_assert_always!` verifies the key is absent in redb

#### Scenario: Uncommitted data not visible
- **WHEN** a write transaction is started but not committed, and a concurrent read transaction opens
- **THEN** `cc_assert_always!` verifies the uncommitted data is not visible

#### Scenario: Liveness assertion
- **THEN** `cc_assert_sometimes!` verifies at least one successful commit occurs per run

#### Scenario: Operation coverage
- **THEN** `cc_assert_reachable!` is called for each operation type to verify coverage

### Requirement: Coverage instrumentation

The guest SHALL call `coverage::init()` at startup and `coverage::record_edge()` after each operation, using the operation type and outcome as edge identifiers. Kernel coverage (KCOV) SHALL be initialized when the kernel supports it.

#### Scenario: Edge coverage per operation
- **WHEN** the guest performs an insert that commits
- **THEN** `coverage::record_edge()` is called with an identifier encoding (operation=insert, outcome=success)

#### Scenario: KCOV integration
- **WHEN** the kernel has CONFIG_KCOV=y
- **THEN** `kcov::init()` succeeds and `kcov::collect()` drains kernel PCs into the coverage bitmap

### Requirement: Key space bounded

The guest SHALL use a bounded key space (keys 0 through 999) so that operations frequently collide and the database stays small enough for fast snapshot/restore.

#### Scenario: Key generation
- **WHEN** the guest generates a key for an operation
- **THEN** the key is `get_random() % 1000` cast to `u64`

#### Scenario: Value generation
- **WHEN** the guest generates a value
- **THEN** the value length is 8 to 64 bytes, chosen via `random_choice()`
- **AND** the value content is deterministic (derived from key and a counter)

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

### Requirement: setup_complete gates fault injection

The guest SHALL call `lifecycle::setup_complete()` after mounting the disk and opening the database, before entering the operation loop.

#### Scenario: Faults gated
- **WHEN** the guest has not yet called `setup_complete()`
- **THEN** the fault engine does not inject disk faults
- **WHEN** `setup_complete()` is called
- **THEN** disk faults (DiskTornWrite, DiskCorruption, DiskSlow, DiskFull) may be injected

### Requirement: Crash recovery verification

After the explorer restores a snapshot (simulating a crash), the guest SHALL re-open the database and verify all previously committed data is intact by comparing against the shadow oracle from the snapshot.

#### Scenario: Post-crash read verification
- **WHEN** the explorer restores a snapshot that includes committed transactions
- **THEN** the guest re-opens the database
- **AND** reads all keys from the oracle and verifies they match in redb

#### Scenario: Post-crash database integrity
- **WHEN** the explorer restores a snapshot after a DiskTornWrite fault
- **THEN** `Database::open()` either succeeds (and data is consistent) or returns a repairable error
- **AND** if repair is needed, `Database::repair()` is called and the database opens afterward
