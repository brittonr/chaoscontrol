## Context

ChaosControl has a single guest workload (Raft) that exercises network faults and multi-VM consensus. The disk fault injection paths (DiskTornWrite, DiskCorruption, DiskSlow, DiskFull) exist but have never been tested against a real storage engine. redb is an ideal target: pure Rust, no system dependencies, ACID transactions with crash safety claims, uses a single file as its backing store. It runs in a single VM with a virtio-blk disk image.

The existing infrastructure already supports everything needed: virtio-blk with CoW snapshots, ext4 disk images via `--disk-image`, fault schedule mutations, and the full exploration/minimize/reproduce pipeline.

## Goals / Non-Goals

**Goals:**
- Guest binary that runs randomized redb operations and asserts ACID properties
- Crash consistency testing: snapshot before crash, verify database recovers correctly
- Exercise disk fault injection paths with real I/O patterns
- Assertions dense enough for the explorer to measure coverage and find violations
- Nix packaging so `nix run .#explore-redb` works end-to-end

**Non-Goals:**
- Performance benchmarking (we care about correctness under faults, not throughput)
- Testing redb's multi-threaded MVCC (single-threaded guest is fine; concurrency comes from crash/restore)
- Custom storage backend (use redb's default file-based backend on ext4)
- Modifying redb source code (treat it as a black-box dependency)

## Decisions

### 1. Single-VM, single-file workload
redb is an embedded database — no network protocol. One VM, one disk image, one database file. The explorer's value here is crash injection + disk faults, not network partitions.

Alternative: wrap redb in a TCP server and use multi-VM. Rejected — adds complexity without testing redb's actual guarantees. The interesting bugs are in crash recovery, not networking.

### 2. ext4 disk image, mounted at boot
The guest mounts the virtio-blk device (formatted ext4) at `/data` and opens `/data/test.redb`. The disk image is pre-formatted at Nix build time (empty ext4). redb creates its database file on first open.

Alternative: use raw block device directly via redb's `StorageBackend` trait. Rejected — redb is designed for filesystem use, and real users run it on ext4/xfs. Testing through the filesystem is more representative.

### 3. Workload structure: operation loop with oracle
Each iteration:
1. Pick operation via `random_choice()`: insert, read, delete, range scan, savepoint, rollback, compact
2. Execute operation against redb
3. Maintain a shadow `BTreeMap<u64, Vec<u8>>` as the oracle
4. Assert redb state matches oracle after every committed transaction
5. Record coverage edges for operation type × outcome

The oracle is the ground truth. If redb diverges from the oracle after a crash+recovery, that's a bug.

### 4. Assertion catalog

| Assertion | Kind | What it checks |
|-----------|------|----------------|
| committed data survives restart | always | After recovery, all committed k/v pairs match oracle |
| uncommitted data not visible | always | After recovery, uncommitted writes are absent |
| range scan matches oracle | always | Range query returns same keys/values as oracle range |
| table len matches oracle | always | `table.len()` == oracle.len() |
| database opens after crash | always | `Database::open()` succeeds (no corruption) |
| delete removes key | always | After delete+commit, key is absent |
| savepoint rollback restores state | always | After rollback, state matches oracle snapshot |
| values committed | sometimes | At least one successful commit per run |
| large transaction committed | sometimes | At least one batch insert (10+ keys) committed |
| all operation types exercised | reachable | Each of the 7 operation types is hit |

### 5. Disk image size
16 MB ext4 image. redb's default page size is 4 KB, and we're doing small key-value pairs. 16 MB gives plenty of room for thousands of transactions while keeping snapshot/restore fast.

## Risks / Trade-offs

**[Risk]** redb may not build with musl static linking.
→ redb is pure Rust with no C dependencies. It should work. If not, investigate `#[cfg]` issues.

**[Risk]** ext4 mount inside the VM may fail or be slow.
→ The net guest already does this pattern (mount, retry loop). Reuse the same approach. The kernel has ext4 built-in.

**[Risk]** CoW block device + DiskTornWrite may not faithfully simulate torn writes at the filesystem layer.
→ DiskTornWrite operates at the block device level (partial sector writes), which is below ext4. This is realistic — it's how real disk failures manifest. ext4's journal should handle it; redb's double-buffered commit should handle it independently.

**[Risk]** redb's crash recovery may take many exits.
→ Recovery walks the B-tree to rebuild allocator state. Budget extra ticks for post-crash branches. Set idle threshold higher if needed.

**[Trade-off]** Shadow oracle in guest memory grows with database size.
→ Keep key space small (keys 0–999, values 8–64 bytes). The oracle is a BTreeMap<u64, Vec<u8>> — a few hundred KB at most.
