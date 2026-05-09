//! Deterministic sweep tests for SnapshotMemory.
//!
//! These preserve the prior property-test invariants without pulling a proc-macro
//! property-test dependency into the dependency-audit surface.

use chaoscontrol_vmm::snapshot::{SnapshotMemory, PAGE_SIZE};
use std::collections::BTreeMap;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const CASES: u64 = 200;

#[derive(Clone)]
struct DeterministicCase {
    state: u64,
}

impl DeterministicCase {
    fn new(index: u64) -> Self {
        Self {
            state: index ^ 0x8cb9_2baa_72f3_d8dd,
        }
    }

    fn next(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(2_862_933_555_777_941_757)
            .wrapping_add(3_037_000_493);
        self.state
    }

    fn usize(&mut self, min: usize, max: usize) -> usize {
        min + (self.next() as usize % (max - min + 1))
    }

    fn u8(&mut self) -> u8 {
        self.next() as u8
    }

    fn bool(&mut self) -> bool {
        self.next() & 1 == 1
    }
}

fn make_pages(num_pages: usize, seed: u8) -> Vec<u8> {
    let mut data = vec![0u8; num_pages * PAGE_SIZE];
    for page in 0..num_pages {
        let fill = seed.wrapping_add(page as u8);
        data[page * PAGE_SIZE..(page + 1) * PAGE_SIZE].fill(fill);
    }
    data
}

fn make_dirty_page(fill: u8) -> Box<[u8; PAGE_SIZE]> {
    Box::new([fill; PAGE_SIZE])
}

fn dirty_flags(tc: &mut DeterministicCase, num_pages: usize) -> Vec<bool> {
    (0..num_pages).map(|_| tc.bool()).collect()
}

#[test]
fn full_materialize_is_identity() {
    for case in 0..CASES {
        let mut tc = DeterministicCase::new(case);
        let num_pages = tc.usize(1, 64);
        let seed = tc.u8();
        let data = make_pages(num_pages, seed);
        let snap = SnapshotMemory::Full(data.clone());
        assert_eq!(snap.materialize(), data, "case {case}");
        assert_eq!(snap.memory_size(), num_pages * PAGE_SIZE);
        assert_eq!(snap.dirty_page_count(), 0);
    }
}

#[test]
fn empty_overlay_equals_base() {
    for case in 0..CASES {
        let mut tc = DeterministicCase::new(case);
        let num_pages = tc.usize(1, 64);
        let seed = tc.u8();
        let base = make_pages(num_pages, seed);

        let snap = SnapshotMemory::Overlay {
            base: Arc::new(base.clone()),
            dirty_pages: BTreeMap::new(),
        };

        assert_eq!(snap.materialize(), base, "case {case}");
        assert_eq!(snap.memory_size(), num_pages * PAGE_SIZE);
        assert_eq!(snap.dirty_page_count(), 0);
    }
}

#[test]
fn overlay_materialize_matches_manual_patch() {
    for case in 0..CASES {
        let mut tc = DeterministicCase::new(case);
        let num_pages = tc.usize(1, 64);
        let base_seed = tc.u8();
        let base = make_pages(num_pages, base_seed);
        let flags = dirty_flags(&mut tc, num_pages);

        let mut dirty_pages = BTreeMap::new();
        for (idx, dirty) in flags.iter().enumerate() {
            if *dirty {
                dirty_pages.insert(idx, make_dirty_page(tc.u8()));
            }
        }

        let snap = SnapshotMemory::Overlay {
            base: Arc::new(base.clone()),
            dirty_pages: dirty_pages.clone(),
        };

        let mut expected = base;
        for (&idx, page_data) in &dirty_pages {
            let offset = idx * PAGE_SIZE;
            expected[offset..offset + PAGE_SIZE].copy_from_slice(page_data.as_ref());
        }

        assert_eq!(snap.materialize(), expected, "case {case}");
        assert_eq!(
            snap.dirty_page_count(),
            flags.iter().filter(|dirty| **dirty).count()
        );
    }
}

#[test]
fn clone_overlay_shares_base_and_matches_content() {
    for case in 0..CASES {
        let mut tc = DeterministicCase::new(case);
        let num_pages = tc.usize(1, 64);
        let base_seed = tc.u8();
        let base = Arc::new(make_pages(num_pages, base_seed));
        let flags = dirty_flags(&mut tc, num_pages);

        let mut dirty_pages = BTreeMap::new();
        for (idx, dirty) in flags.iter().enumerate() {
            if *dirty {
                dirty_pages.insert(idx, make_dirty_page(tc.u8()));
            }
        }

        let snap = SnapshotMemory::Overlay {
            base: Arc::clone(&base),
            dirty_pages,
        };
        let cloned = snap.clone();

        assert_eq!(snap.materialize(), cloned.materialize(), "case {case}");

        if let (
            SnapshotMemory::Overlay { base: b1, .. },
            SnapshotMemory::Overlay { base: b2, .. },
        ) = (&snap, &cloned)
        {
            assert!(
                Arc::ptr_eq(b1, b2),
                "case {case}: clone must share base Arc"
            );
        }
    }
}

#[test]
fn from_dirty_round_trips_through_guest_memory() {
    for case in 0..CASES {
        let mut tc = DeterministicCase::new(case);
        let num_pages = tc.usize(1, 32);
        let size = num_pages * PAGE_SIZE;

        let guest_mem =
            GuestMemoryMmap::from_ranges(&[(GuestAddress(0), size)]).expect("create guest memory");

        let mut page_fills = Vec::with_capacity(num_pages);
        for page in 0..num_pages {
            let fill = tc.u8();
            page_fills.push(fill);
            let data = vec![fill; PAGE_SIZE];
            guest_mem
                .write_slice(&data, GuestAddress((page * PAGE_SIZE) as u64))
                .unwrap();
        }

        let base = Arc::new(vec![0u8; size]);
        let flags = dirty_flags(&mut tc, num_pages);

        let mut bitmap_words = vec![0u64; num_pages.div_ceil(64)];
        for (i, dirty) in flags.iter().enumerate() {
            if *dirty {
                bitmap_words[i / 64] |= 1u64 << (i % 64);
            }
        }

        let snap = SnapshotMemory::from_dirty(&base, &bitmap_words, &guest_mem);

        let SnapshotMemory::Overlay {
            base: snap_base,
            dirty_pages,
        } = &snap
        else {
            panic!("from_dirty should produce Overlay");
        };

        assert!(Arc::ptr_eq(snap_base, &base));

        for (i, dirty) in flags.iter().enumerate() {
            if *dirty {
                assert!(
                    dirty_pages.contains_key(&i),
                    "case {case}: page {i} missing"
                );
                assert!(
                    dirty_pages[&i].iter().all(|byte| *byte == page_fills[i]),
                    "case {case}: page {i} content mismatch"
                );
            } else {
                assert!(
                    !dirty_pages.contains_key(&i),
                    "case {case}: page {i} unexpected"
                );
            }
        }
    }
}

#[test]
fn write_to_guest_then_read_back_matches_materialize() {
    for case in 0..CASES {
        let mut tc = DeterministicCase::new(case);
        let num_pages = tc.usize(1, 32);
        let size = num_pages * PAGE_SIZE;
        let base_seed = tc.u8();
        let base = make_pages(num_pages, base_seed);
        let flags = dirty_flags(&mut tc, num_pages);

        let mut dirty_pages = BTreeMap::new();
        for (idx, dirty) in flags.iter().enumerate() {
            if *dirty {
                dirty_pages.insert(idx, make_dirty_page(tc.u8()));
            }
        }

        let snap = SnapshotMemory::Overlay {
            base: Arc::new(base.clone()),
            dirty_pages,
        };

        let expected = snap.materialize();
        let guest_mem =
            GuestMemoryMmap::from_ranges(&[(GuestAddress(0), size)]).expect("create guest memory");
        guest_mem
            .write_slice(&base, GuestAddress(0))
            .expect("write base");
        snap.write_to_guest(&guest_mem).expect("write overlay");

        let mut readback = vec![0u8; size];
        guest_mem
            .read_slice(&mut readback, GuestAddress(0))
            .expect("read back");

        assert_eq!(readback, expected, "case {case}");
    }
}

#[test]
fn full_and_overlay_with_all_dirty_produce_same_result() {
    for case in 0..CASES {
        let mut tc = DeterministicCase::new(case);
        let num_pages = tc.usize(1, 32);
        let base_seed = tc.u8();
        let base = make_pages(num_pages, base_seed);
        let dirty_seed = tc.u8();
        let full_data = make_pages(num_pages, dirty_seed);

        let mut dirty_pages = BTreeMap::new();
        for page in 0..num_pages {
            let mut arr = Box::new([0u8; PAGE_SIZE]);
            arr.copy_from_slice(&full_data[page * PAGE_SIZE..(page + 1) * PAGE_SIZE]);
            dirty_pages.insert(page, arr);
        }

        let full_snap = SnapshotMemory::Full(full_data.clone());
        let overlay_snap = SnapshotMemory::Overlay {
            base: Arc::new(base),
            dirty_pages,
        };

        assert_eq!(
            full_snap.materialize(),
            overlay_snap.materialize(),
            "case {case}: full and all-dirty overlay must produce same result"
        );
    }
}
