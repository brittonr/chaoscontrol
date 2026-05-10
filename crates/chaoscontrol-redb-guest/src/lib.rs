//! Pure logic for the redb guest — shadow oracle, operation types,
//! and verification helpers.  No SDK dependency so it's unit-testable.

use std::collections::BTreeMap;
use std::ops::RangeBounds;

// ═══════════════════════════════════════════════════════════════════════
//  Operation types
// ═══════════════════════════════════════════════════════════════════════

/// Operations the workload can perform against redb.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Insert,
    Read,
    Delete,
    RangeScan,
    Savepoint,
    Rollback,
    Compact,
}

impl Op {
    pub const COUNT: usize = 7;

    pub fn from_index(i: usize) -> Self {
        match i % Self::COUNT {
            0 => Self::Insert,
            1 => Self::Read,
            2 => Self::Delete,
            3 => Self::RangeScan,
            4 => Self::Savepoint,
            5 => Self::Rollback,
            6 => Self::Compact,
            _ => unreachable!(),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Insert => "insert",
            Self::Read => "read",
            Self::Delete => "delete",
            Self::RangeScan => "range_scan",
            Self::Savepoint => "savepoint",
            Self::Rollback => "rollback",
            Self::Compact => "compact",
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Key/value helpers
// ═══════════════════════════════════════════════════════════════════════

/// Maximum key (exclusive).  Keys are 0..MAX_KEY.
pub const MAX_KEY: u64 = 1000;

/// Build a deterministic value from key + sequence counter.
/// Length is 8..64 bytes controlled by `size_choice` (0..7).
pub fn make_value(key: u64, seq: u64, size_choice: usize) -> Vec<u8> {
    let len = 8 + (size_choice % 8) * 8; // 8, 16, 24, …, 64
    let mut v = Vec::with_capacity(len);
    // First 8 bytes: key XOR seq as little-endian.
    v.extend_from_slice(&(key ^ seq).to_le_bytes());
    // Fill the rest with a repeating pattern.
    while v.len() < len {
        let b = ((key.wrapping_add(v.len() as u64)) & 0xFF) as u8;
        v.push(b);
    }
    v
}

// ═══════════════════════════════════════════════════════════════════════
//  Shadow oracle
// ═══════════════════════════════════════════════════════════════════════

/// Ground-truth model for committed redb state.
///
/// Updated only when a transaction actually commits so that
/// post-crash verification can compare redb contents against
/// what *should* have survived.
#[derive(Debug, Clone)]
pub struct Oracle {
    map: BTreeMap<u64, Vec<u8>>,
    /// Saved snapshot for savepoint/rollback.
    snapshot: Option<BTreeMap<u64, Vec<u8>>>,
}

impl Oracle {
    pub fn new() -> Self {
        Self {
            map: BTreeMap::new(),
            snapshot: None,
        }
    }

    pub fn insert(&mut self, key: u64, value: Vec<u8>) {
        self.map.insert(key, value);
    }

    pub fn delete(&mut self, key: u64) -> bool {
        self.map.remove(&key).is_some()
    }

    pub fn get(&self, key: u64) -> Option<&Vec<u8>> {
        self.map.get(&key)
    }

    pub fn range<R: RangeBounds<u64>>(&self, bounds: R) -> Vec<(u64, Vec<u8>)> {
        self.map
            .range(bounds)
            .map(|(&k, v)| (k, v.clone()))
            .collect()
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Pick a key that exists, given a raw u64 used as an index.
    /// Returns None if oracle is empty.
    pub fn pick_existing_key(&self, raw: u64) -> Option<u64> {
        if self.map.is_empty() {
            return None;
        }
        let keys: Vec<u64> = self.map.keys().copied().collect();
        Some(keys[(raw as usize) % keys.len()])
    }

    /// All keys, sorted.
    pub fn keys(&self) -> Vec<u64> {
        self.map.keys().copied().collect()
    }

    // ── Savepoint / rollback ──────────────────────────────────────────

    pub fn save_snapshot(&mut self) {
        self.snapshot = Some(self.map.clone());
    }

    pub fn has_snapshot(&self) -> bool {
        self.snapshot.is_some()
    }

    pub fn restore_snapshot(&mut self) -> bool {
        if let Some(snap) = self.snapshot.take() {
            self.map = snap;
            true
        } else {
            false
        }
    }
}

impl Default for Oracle {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oracle_insert_get_delete() {
        let mut o = Oracle::new();
        assert!(o.is_empty());

        o.insert(42, vec![1, 2, 3]);
        assert_eq!(o.len(), 1);
        assert_eq!(o.get(42), Some(&vec![1, 2, 3]));

        assert!(o.delete(42));
        assert!(o.is_empty());
        assert_eq!(o.get(42), None);
        assert!(!o.delete(42)); // already gone
    }

    #[test]
    fn oracle_range() {
        let mut o = Oracle::new();
        for k in 0..10 {
            o.insert(k, vec![k as u8]);
        }
        let r = o.range(3..7);
        assert_eq!(r.len(), 4);
        assert_eq!(r[0], (3, vec![3]));
        assert_eq!(r[3], (6, vec![6]));
    }

    #[test]
    fn oracle_snapshot_restore() {
        let mut o = Oracle::new();
        o.insert(1, vec![10]);
        o.insert(2, vec![20]);
        o.save_snapshot();

        o.insert(3, vec![30]);
        o.delete(1);
        assert_eq!(o.len(), 2); // keys 2, 3

        assert!(o.restore_snapshot());
        assert_eq!(o.len(), 2); // keys 1, 2
        assert_eq!(o.get(1), Some(&vec![10]));
        assert_eq!(o.get(3), None);
    }

    #[test]
    fn oracle_restore_without_snapshot() {
        let mut o = Oracle::new();
        assert!(!o.restore_snapshot());
    }

    #[test]
    fn oracle_pick_existing_key() {
        let mut o = Oracle::new();
        assert_eq!(o.pick_existing_key(0), None);

        o.insert(10, vec![]);
        o.insert(20, vec![]);
        o.insert(30, vec![]);

        let k = o.pick_existing_key(0).unwrap();
        assert!([10, 20, 30].contains(&k));
    }

    #[test]
    fn make_value_lengths() {
        for choice in 0..8 {
            let v = make_value(42, 1, choice);
            assert_eq!(v.len(), 8 + choice * 8);
        }
    }

    #[test]
    fn redb_local_assertion_harness_covers_readiness_gap_conditions() {
        use redb::{Database, ReadableDatabase, ReadableTableMetadata, TableDefinition};

        const TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("kv");

        let db_path = std::env::temp_dir().join(format!(
            "chaoscontrol-redb-local-harness-{}-{}.redb",
            std::process::id(),
            option_env!("NEXTEST_TEST_GROUP").unwrap_or("unit")
        ));
        let _ = std::fs::remove_file(&db_path);

        let db = Database::create(&db_path).expect("database opens after repair harness path");
        let empty_oracle = Oracle::new();
        let read_txn = db.begin_read().expect("begin empty read");
        assert!(
            read_txn.open_table(TABLE).is_err(),
            "read matches oracle (no table)"
        );
        assert!(
            empty_oracle.range(0..100).is_empty(),
            "range scan empty table matches oracle"
        );
        assert!(
            empty_oracle.is_empty(),
            "table len matches oracle (no table)"
        );
        drop(read_txn);

        let mut oracle = Oracle::new();
        let committed_value = make_value(7, 1, 2);
        {
            let write_txn = db.begin_write().expect("begin committed write");
            {
                let mut table = write_txn.open_table(TABLE).expect("open write table");
                table
                    .insert(7, committed_value.as_slice())
                    .expect("insert committed value");
            }
            write_txn.commit().expect("commit value");
            oracle.insert(7, committed_value.clone());
        }
        {
            let write_txn = db.begin_write().expect("begin uncommitted write");
            {
                let mut table = write_txn.open_table(TABLE).expect("open uncommitted table");
                table
                    .insert(8, &[8_u8][..])
                    .expect("insert uncommitted value");
            }
            drop(write_txn);
        }
        {
            let read_txn = db.begin_read().expect("begin visibility read");
            let table = read_txn.open_table(TABLE).expect("open visibility table");
            let table: redb::ReadOnlyTable<u64, &[u8]> = table;
            assert!(table.get(8).expect("read uncommitted key").is_none());
        }
        drop(db);

        let mut reopened = Database::create(&db_path).expect("committed data survives restart");
        {
            let read_txn = reopened.begin_read().expect("begin restart read");
            let table = read_txn.open_table(TABLE).expect("open restart table");
            let table: redb::ReadOnlyTable<u64, &[u8]> = table;
            let actual = table
                .get(7)
                .expect("read committed key")
                .expect("committed key missing after recovery")
                .value()
                .to_vec();
            assert_eq!(actual, committed_value, "committed data survives restart");
            assert_eq!(table.len().expect("table len") as usize, oracle.len());
        }
        reopened.compact().expect("compact database");
        {
            let read_txn = reopened.begin_read().expect("begin compact read");
            let table = read_txn.open_table(TABLE).expect("open compact table");
            let table: redb::ReadOnlyTable<u64, &[u8]> = table;
            let actual = table
                .get(7)
                .expect("read compacted key")
                .expect("compacted key exists")
                .value()
                .to_vec();
            assert_eq!(actual, committed_value, "data survives compaction");
        }
        drop(reopened);

        let repaired = redb::Builder::new()
            .create(&db_path)
            .expect("database opens after repair");
        drop(repaired);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn make_value_deterministic() {
        let a = make_value(42, 7, 3);
        let b = make_value(42, 7, 3);
        assert_eq!(a, b);
    }

    #[test]
    fn op_from_index_covers_all() {
        let ops: Vec<Op> = (0..Op::COUNT).map(Op::from_index).collect();
        assert_eq!(ops.len(), 7);
        assert!(ops.contains(&Op::Insert));
        assert!(ops.contains(&Op::Compact));
    }
}
