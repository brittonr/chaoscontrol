//! redb crash-consistency testing guest for ChaosControl.
//!
//! Runs as PID 1 inside a ChaosControl VM.  Opens a redb database on
//! the virtio-blk disk, executes randomised key-value operations, and
//! asserts ACID properties against a shadow oracle.

use chaoscontrol_redb_guest::{make_value, Op, Oracle, MAX_KEY};
use chaoscontrol_sdk::prelude::*;
use chaoscontrol_sdk::{coverage, kcov, lifecycle, random};
use redb::{Database, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition};
use serde_json::json;

const TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("kv");
const DB_PATH: &str = "/data/test.redb";

fn cmdline_value(name: &str) -> Option<String> {
    let cmdline = std::fs::read_to_string("/proc/cmdline").unwrap_or_default();
    let prefix = format!("{name}=");
    cmdline
        .split_whitespace()
        .find_map(|token| token.strip_prefix(&prefix).map(str::to_owned))
}

fn snapshot_probe_enabled() -> bool {
    matches!(
        cmdline_value("redb_bug").as_deref(),
        Some("snapshot_replay_probe") | Some("snapshot_probe")
    )
}

fn snapshot_probe_fail_after() -> u64 {
    cmdline_value("redb_snapshot_probe_fail_after")
        .and_then(|value| value.parse().ok())
        .unwrap_or(25)
}

// ═══════════════════════════════════════════════════════════════════════
//  Mount helpers
// ═══════════════════════════════════════════════════════════════════════

fn mount_devtmpfs() {
    unsafe {
        libc::mount(
            c"devtmpfs".as_ptr(),
            c"/dev".as_ptr(),
            c"devtmpfs".as_ptr(),
            0,
            std::ptr::null(),
        );
    }
}

fn mount_sysfs() {
    unsafe {
        libc::mount(
            c"sysfs".as_ptr(),
            c"/sys".as_ptr(),
            c"sysfs".as_ptr(),
            0,
            std::ptr::null(),
        );
    }
}

/// Mount /dev/vda as ext4 on /data.  Retries because the virtio-blk
/// device may take a moment to appear.
fn mount_disk() {
    std::fs::create_dir_all("/data").ok();
    for attempt in 0..50 {
        let rc = unsafe {
            libc::mount(
                c"/dev/vda".as_ptr(),
                c"/data".as_ptr(),
                c"ext4".as_ptr(),
                0,
                std::ptr::null(),
            )
        };
        if rc == 0 {
            println!(
                "redb-guest: mounted /dev/vda on /data (attempt {})",
                attempt
            );
            return;
        }
        // Small busy-wait (no real sleep in the VM).
        for _ in 0..100_000 {
            core::hint::spin_loop();
        }
    }
    println!("redb-guest: WARNING — failed to mount /dev/vda, continuing anyway");
}

// ═══════════════════════════════════════════════════════════════════════
//  Database helpers
// ═══════════════════════════════════════════════════════════════════════

/// Open or create the database.  On corruption, attempt repair first.
fn open_database() -> Option<Database> {
    match Database::create(DB_PATH) {
        Ok(db) => {
            cc_assert_always_category!("redb", "invariant", true, "database opens successfully");
            Some(db)
        }
        Err(e) => {
            println!("redb-guest: Database::create failed: {e}");
            // Try repair.
            println!("redb-guest: attempting repair…");
            match redb::Builder::new().create(DB_PATH) {
                Ok(db) => {
                    println!("redb-guest: database recovered after repair");
                    cc_assert_always_category!(
                        "redb",
                        "invariant",
                        true,
                        "database opens after repair"
                    );
                    Some(db)
                }
                Err(e3) => {
                    println!("redb-guest: open after repair failed: {e3}");
                    cc_assert_always_category!(
                        "redb",
                        "invariant",
                        false,
                        "database opens after repair"
                    );
                    None
                }
            }
        }
    }
}

/// Full verification: read every oracle key from redb and assert match.
fn verify_all(db: &Database, oracle: &Oracle) {
    let read_txn = match db.begin_read() {
        Ok(t) => t,
        Err(e) => {
            println!("redb-guest: verify begin_read failed: {e}");
            return;
        }
    };
    let table: redb::ReadOnlyTable<u64, &[u8]> = match read_txn.open_table(TABLE) {
        Ok(t) => t,
        Err(redb::TableError::TableDoesNotExist(_)) => return, // empty db is fine
        Err(e) => {
            println!("redb-guest: verify open_table failed: {e}");
            return;
        }
    };

    for key in oracle.keys() {
        let expected = oracle.get(key).unwrap();
        match table.get(key) {
            Ok(Some(guard)) => {
                let got: &[u8] = guard.value();
                cc_assert_always_category!(
                    "redb",
                    "invariant",
                    got == expected.as_slice(),
                    "committed data survives restart",
                );
            }
            Ok(None) => {
                cc_assert_always_category!(
                    "redb",
                    "invariant",
                    false,
                    "committed key missing after recovery"
                );
            }
            Err(e) => {
                println!("redb-guest: verify get({key}) error: {e}");
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Operation handlers
// ═══════════════════════════════════════════════════════════════════════

fn do_insert(db: &Database, oracle: &mut Oracle, seq: &mut u64) {
    cc_assert_reachable_category!("redb", "operation", "op: insert");

    let key = random::get_random() % MAX_KEY;
    let size_choice = random::random_choice(8);
    let value = make_value(key, *seq, size_choice);
    *seq += 1;

    let write_txn = match db.begin_write() {
        Ok(t) => t,
        Err(e) => {
            println!("redb-guest: insert begin_write: {e}");
            return;
        }
    };
    {
        let mut table = match write_txn.open_table(TABLE) {
            Ok(t) => t,
            Err(e) => {
                println!("redb-guest: insert open_table: {e}");
                return;
            }
        };
        if let Err(e) = table.insert(key, value.as_slice()) {
            println!("redb-guest: insert({key}): {e}");
            return;
        };
    }
    match write_txn.commit() {
        Ok(()) => {
            oracle.insert(key, value);
            cc_assert_sometimes_category!("redb", "operation", true, "commit succeeds");
            coverage::record_edge(0x1000);
        }
        Err(e) => {
            println!("redb-guest: insert commit: {e}");
            coverage::record_edge(0x1001);
        }
    }
}

fn do_batch_insert(db: &Database, oracle: &mut Oracle, seq: &mut u64) {
    cc_assert_reachable_category!("redb", "operation", "op: batch insert");

    let count = 10 + random::random_choice(11); // 10..20
    let mut pairs = Vec::with_capacity(count);
    for _ in 0..count {
        let key = random::get_random() % MAX_KEY;
        let size_choice = random::random_choice(8);
        let value = make_value(key, *seq, size_choice);
        *seq += 1;
        pairs.push((key, value));
    }

    let write_txn = match db.begin_write() {
        Ok(t) => t,
        Err(e) => {
            println!("redb-guest: batch begin_write: {e}");
            return;
        }
    };
    {
        let mut table = match write_txn.open_table(TABLE) {
            Ok(t) => t,
            Err(e) => {
                println!("redb-guest: batch open_table: {e}");
                return;
            }
        };
        for (k, v) in &pairs {
            if let Err(e) = table.insert(*k, v.as_slice()) {
                println!("redb-guest: batch insert({k}): {e}");
                return;
            }
        }
    }
    match write_txn.commit() {
        Ok(()) => {
            for (k, v) in pairs {
                oracle.insert(k, v);
            }
            cc_assert_sometimes_category!("redb", "operation", true, "large batch committed");
            coverage::record_edge(0x1010);
        }
        Err(e) => {
            println!("redb-guest: batch commit: {e}");
            coverage::record_edge(0x1011);
        }
    }
}

fn do_read(db: &Database, oracle: &Oracle) {
    cc_assert_reachable_category!("redb", "operation", "op: read");

    let key = random::get_random() % MAX_KEY;
    let read_txn = match db.begin_read() {
        Ok(t) => t,
        Err(e) => {
            println!("redb-guest: read begin_read: {e}");
            return;
        }
    };
    let table: redb::ReadOnlyTable<u64, &[u8]> = match read_txn.open_table(TABLE) {
        Ok(t) => t,
        Err(redb::TableError::TableDoesNotExist(_)) => {
            // Empty database — oracle should also be empty for this key.
            cc_assert_always_category!(
                "redb",
                "invariant",
                oracle.get(key).is_none(),
                "read matches oracle (no table)",
            );
            coverage::record_edge(0x2000);
            return;
        }
        Err(e) => {
            println!("redb-guest: read open_table: {e}");
            return;
        }
    };

    match table.get(key) {
        Ok(Some(guard)) => {
            let got: &[u8] = guard.value();
            match oracle.get(key) {
                Some(expected) => {
                    cc_assert_always_category!(
                        "redb",
                        "invariant",
                        got == expected.as_slice(),
                        "read matches oracle",
                    );
                }
                None => {
                    // redb has key but oracle doesn't — uncommitted data visible?
                    cc_assert_always_category!(
                        "redb",
                        "invariant",
                        false,
                        "uncommitted data not visible"
                    );
                }
            }
            coverage::record_edge(0x2001);
        }
        Ok(None) => {
            cc_assert_always_category!(
                "redb",
                "invariant",
                oracle.get(key).is_none(),
                "read matches oracle (none)",
            );
            coverage::record_edge(0x2002);
        }
        Err(e) => {
            println!("redb-guest: read get({key}): {e}");
            coverage::record_edge(0x2003);
        }
    }
}

fn do_delete(db: &Database, oracle: &mut Oracle) {
    cc_assert_reachable_category!("redb", "operation", "op: delete");

    let raw = random::get_random();
    let key = match oracle.pick_existing_key(raw) {
        Some(k) => k,
        None => {
            // Oracle empty — delete a random key (should be no-op).
            raw % MAX_KEY
        }
    };

    let write_txn = match db.begin_write() {
        Ok(t) => t,
        Err(e) => {
            println!("redb-guest: delete begin_write: {e}");
            return;
        }
    };
    {
        let mut table = match write_txn.open_table(TABLE) {
            Ok(t) => t,
            Err(e) => {
                println!("redb-guest: delete open_table: {e}");
                return;
            }
        };
        if let Err(e) = table.remove(key) {
            println!("redb-guest: delete({key}): {e}");
            return;
        };
    }
    match write_txn.commit() {
        Ok(()) => {
            oracle.delete(key);
            // Verify deletion: read back.
            if let Ok(rtx) = db.begin_read() {
                if let Ok(t) = rtx.open_table(TABLE) {
                    let t: redb::ReadOnlyTable<u64, &[u8]> = t;
                    if let Ok(val) = t.get(key) {
                        cc_assert_always_category!(
                            "redb",
                            "invariant",
                            val.is_none(),
                            "delete removes key"
                        );
                    }
                }
            }
            coverage::record_edge(0x3000);
        }
        Err(e) => {
            println!("redb-guest: delete commit: {e}");
            coverage::record_edge(0x3001);
        }
    }
}

fn do_range_scan(db: &Database, oracle: &Oracle) {
    cc_assert_reachable_category!("redb", "operation", "op: range scan");

    let a = random::get_random() % MAX_KEY;
    let b = random::get_random() % MAX_KEY;
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };

    let expected = oracle.range(lo..hi);

    let read_txn = match db.begin_read() {
        Ok(t) => t,
        Err(e) => {
            println!("redb-guest: range begin_read: {e}");
            return;
        }
    };
    let table: redb::ReadOnlyTable<u64, &[u8]> = match read_txn.open_table(TABLE) {
        Ok(t) => t,
        Err(redb::TableError::TableDoesNotExist(_)) => {
            cc_assert_always_category!(
                "redb",
                "invariant",
                expected.is_empty(),
                "range scan empty table matches oracle"
            );
            coverage::record_edge(0x4000);
            return;
        }
        Err(e) => {
            println!("redb-guest: range open_table: {e}");
            return;
        }
    };

    let mut actual = Vec::new();
    match table.range(lo..hi) {
        Ok(iter) => {
            for entry in iter {
                match entry {
                    Ok(pair) => {
                        actual.push((pair.0.value(), pair.1.value().to_vec()));
                    }
                    Err(e) => {
                        println!("redb-guest: range iter: {e}");
                        return;
                    }
                }
            }
        }
        Err(e) => {
            println!("redb-guest: range: {e}");
            return;
        }
    }

    cc_assert_always_category!(
        "redb",
        "invariant",
        actual.len() == expected.len(),
        "range scan length matches oracle",
    );
    for (i, ((ak, av), (ek, ev))) in actual.iter().zip(expected.iter()).enumerate() {
        cc_assert_always_category!(
            "redb",
            "invariant",
            ak == ek && av == ev,
            "range scan entry matches oracle",
        );
        if ak != ek || av != ev {
            println!(
                "redb-guest: range mismatch at {i}: redb=({ak},{:?}) oracle=({ek},{ev:?})",
                av
            );
        }
    }
    coverage::record_edge(0x4001);
}

fn do_compact(db: &mut Database, oracle: &Oracle) {
    cc_assert_reachable_category!("redb", "operation", "op: compact");

    if let Err(e) = db.compact() {
        println!("redb-guest: compact: {e}");
        coverage::record_edge(0x6001);
        return;
    }

    // Verify a sample key survives compaction.
    if let Some(&first_key) = oracle.keys().first() {
        if let Ok(rtx) = db.begin_read() {
            if let Ok(t) = rtx.open_table(TABLE) {
                let t: redb::ReadOnlyTable<u64, &[u8]> = t;
                if let Ok(Some(guard)) = t.get(first_key) {
                    let expected = oracle.get(first_key).unwrap();
                    let got: &[u8] = guard.value();
                    cc_assert_always_category!(
                        "redb",
                        "invariant",
                        got == expected.as_slice(),
                        "data survives compaction",
                    );
                }
            }
        }
    }
    coverage::record_edge(0x6000);
}

fn check_table_len(db: &Database, oracle: &Oracle) {
    let read_txn = match db.begin_read() {
        Ok(t) => t,
        Err(_) => return,
    };
    let table: redb::ReadOnlyTable<u64, &[u8]> = match read_txn.open_table(TABLE) {
        Ok(t) => t,
        Err(redb::TableError::TableDoesNotExist(_)) => {
            cc_assert_always_category!(
                "redb",
                "invariant",
                oracle.is_empty(),
                "table len matches oracle (no table)"
            );
            return;
        }
        Err(_) => return,
    };
    match table.len() {
        Ok(len) => {
            cc_assert_always_category!(
                "redb",
                "invariant",
                len as usize == oracle.len(),
                "table len matches oracle",
            );
        }
        Err(e) => {
            println!("redb-guest: table.len(): {e}");
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Main
// ═══════════════════════════════════════════════════════════════════════

fn main() {
    guest_init();

    println!("redb-guest: starting (key space 0..{MAX_KEY})");

    // ── Mount filesystems ────────────────────────────────────────────
    mount_devtmpfs();
    mount_sysfs();
    mount_disk();

    // ── Initialise coverage + KCOV ───────────────────────────────────
    coverage::init();
    let _ = kcov::init();

    // ── Open (or recover) the database ───────────────────────────────
    let mut db = match open_database() {
        Some(db) => db,
        None => {
            println!("redb-guest: cannot open database, halting");
            loop {
                core::hint::spin_loop();
            }
        }
    };

    // ── Oracle: if database already has data (post-crash), rebuild ───
    let mut oracle = Oracle::new();
    rebuild_oracle_from_db(&db, &mut oracle);

    if !oracle.is_empty() {
        println!(
            "redb-guest: recovered {} keys from existing database",
            oracle.len()
        );
        // Verify recovered data matches what redb has.
        verify_all(&db, &oracle);
    }

    // ── Signal ready ─────────────────────────────────────────────────
    lifecycle::setup_complete(&json!({"program": "redb-guest", "keys": MAX_KEY}));
    println!("redb-guest: setup_complete");

    // ── Main loop ────────────────────────────────────────────────────
    let mut seq: u64 = oracle.len() as u64; // resume counter after crash
    let mut iter: u64 = 0;
    let mut savepoint_id: Option<u64> = None;
    let snapshot_probe = snapshot_probe_enabled();
    let snapshot_probe_fail_after = snapshot_probe_fail_after();
    if snapshot_probe {
        println!(
            "redb-guest: snapshot replay probe enabled (fail_after={snapshot_probe_fail_after})"
        );
    }

    loop {
        let op_idx = random::random_choice(Op::COUNT + 1); // +1 for batch insert
        let op = if op_idx < Op::COUNT {
            Op::from_index(op_idx)
        } else {
            // Extra slot for batch insert.
            Op::Insert // re-use enum; handler picks batch path below
        };
        let is_batch = op_idx >= Op::COUNT;

        match op {
            Op::Insert => {
                if is_batch {
                    do_batch_insert(&db, &mut oracle, &mut seq);
                } else {
                    do_insert(&db, &mut oracle, &mut seq);
                }
            }
            Op::Read => do_read(&db, &oracle),
            Op::Delete => do_delete(&db, &mut oracle),
            Op::RangeScan => do_range_scan(&db, &oracle),
            Op::Savepoint => {
                cc_assert_reachable_category!("redb", "operation", "op: savepoint");
                match db.begin_write() {
                    Ok(txn) => match txn.persistent_savepoint() {
                        Ok(sp) => {
                            let id = sp;
                            if let Err(e) = txn.commit() {
                                println!("redb-guest: savepoint commit: {e}");
                            } else {
                                oracle.save_snapshot();
                                savepoint_id = Some(id);
                                println!(
                                    "redb-guest: savepoint created id={id} (oracle len={})",
                                    oracle.len()
                                );
                                coverage::record_edge(0x5000);
                            }
                        }
                        Err(e) => {
                            println!("redb-guest: savepoint: {e}");
                            coverage::record_edge(0x5001);
                        }
                    },
                    Err(e) => println!("redb-guest: savepoint begin_write: {e}"),
                }
            }
            Op::Rollback => {
                cc_assert_reachable_category!("redb", "operation", "op: rollback");
                if let Some(id) = savepoint_id.take() {
                    match db.begin_write() {
                        Ok(mut txn) => match txn.get_persistent_savepoint(id) {
                            Ok(sp) => match txn.restore_savepoint(&sp) {
                                Ok(()) => {
                                    if let Err(e) = txn.commit() {
                                        println!("redb-guest: rollback commit: {e}");
                                    } else {
                                        oracle.restore_snapshot();
                                        println!("redb-guest: rollback to savepoint id={id} (oracle len={})", oracle.len());
                                        coverage::record_edge(0x5010);
                                    }
                                }
                                Err(e) => {
                                    println!("redb-guest: restore_savepoint: {e}");
                                    coverage::record_edge(0x5011);
                                }
                            },
                            Err(e) => {
                                println!("redb-guest: get_persistent_savepoint({id}): {e}");
                                coverage::record_edge(0x5013);
                            }
                        },
                        Err(e) => println!("redb-guest: rollback begin_write: {e}"),
                    }
                } else {
                    coverage::record_edge(0x5012);
                }
            }
            Op::Compact => do_compact(&mut db, &oracle),
        }

        // Periodic full checks.
        iter += 1;
        if snapshot_probe {
            cc_assert_always_stable!(
                "org.onixresearch.chaoscontrol.redb",
                "snapshot-replay-probe",
                "redb",
                "recovery",
                iter < snapshot_probe_fail_after,
                "redb snapshot replay probe trips only after restored parent context",
                &json!({"iter": iter, "fail_after": snapshot_probe_fail_after}),
            );
        }
        if iter.is_multiple_of(10) {
            check_table_len(&db, &oracle);
        }
        if iter.is_multiple_of(50) {
            kcov::collect();
        }
    }
}

/// Rebuild the oracle from an existing database (for crash recovery).
fn rebuild_oracle_from_db(db: &Database, oracle: &mut Oracle) {
    let read_txn = match db.begin_read() {
        Ok(t) => t,
        Err(_) => return,
    };
    let table: redb::ReadOnlyTable<u64, &[u8]> = match read_txn.open_table(TABLE) {
        Ok(t) => t,
        Err(_) => return,
    };
    let iter = match table.iter() {
        Ok(it) => it,
        Err(_) => return,
    };
    for pair in iter.flatten() {
        oracle.insert(pair.0.value(), pair.1.value().to_vec());
    }
}
