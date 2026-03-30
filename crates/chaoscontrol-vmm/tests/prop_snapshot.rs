//! Property-based tests for SnapshotMemory.
//!
//! Key properties:
//! - Overlay with no dirty pages materializes identically to the base.
//! - Overlay materialize is equivalent to manually patching dirty pages onto base.
//! - from_dirty round-trips correctly through GuestMemoryMmap.
//! - Clone of overlay shares the Arc base and produces identical materialized content.
//! - Full materialize is identity.
//! - write_to_guest + read_back == materialize.

use chaoscontrol_vmm::snapshot::{SnapshotMemory, PAGE_SIZE};
use hegel::generators::*;
use hegel::TestCase;
use std::collections::BTreeMap;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

/// Build page data from a seed byte — fills every byte with a pattern
/// derived from the seed and page index. Cheap to generate, sufficient
/// for testing structural correctness.
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

#[hegel::test(test_cases = 200)]
fn full_materialize_is_identity(tc: TestCase) {
    let num_pages = tc.draw(integers::<usize>().min_value(1).max_value(64));
    let seed = tc.draw(integers::<u8>());
    let data = make_pages(num_pages, seed);
    let snap = SnapshotMemory::Full(data.clone());
    assert_eq!(snap.materialize(), data);
    assert_eq!(snap.memory_size(), num_pages * PAGE_SIZE);
    assert_eq!(snap.dirty_page_count(), 0);
}

#[hegel::test(test_cases = 200)]
fn empty_overlay_equals_base(tc: TestCase) {
    let num_pages = tc.draw(integers::<usize>().min_value(1).max_value(64));
    let seed = tc.draw(integers::<u8>());
    let base = make_pages(num_pages, seed);

    let snap = SnapshotMemory::Overlay {
        base: Arc::new(base.clone()),
        dirty_pages: BTreeMap::new(),
    };

    assert_eq!(snap.materialize(), base);
    assert_eq!(snap.memory_size(), num_pages * PAGE_SIZE);
    assert_eq!(snap.dirty_page_count(), 0);
}

#[hegel::test(test_cases = 200)]
fn overlay_materialize_matches_manual_patch(tc: TestCase) {
    let num_pages = tc.draw(integers::<usize>().min_value(1).max_value(64));
    let base_seed = tc.draw(integers::<u8>());
    let base = make_pages(num_pages, base_seed);

    // Pick which pages are dirty
    let dirty_flags: Vec<bool> = tc.draw(vecs(booleans()).min_size(num_pages).max_size(num_pages));

    // Build dirty pages with distinct fill bytes
    let mut dirty_pages = BTreeMap::new();
    for (idx, &dirty) in dirty_flags.iter().enumerate() {
        if dirty {
            let fill = tc.draw(integers::<u8>());
            dirty_pages.insert(idx, make_dirty_page(fill));
        }
    }

    let snap = SnapshotMemory::Overlay {
        base: Arc::new(base.clone()),
        dirty_pages: dirty_pages.clone(),
    };

    // Manual patch: start from base, overwrite dirty pages
    let mut expected = base;
    for (&idx, page_data) in &dirty_pages {
        let offset = idx * PAGE_SIZE;
        expected[offset..offset + PAGE_SIZE].copy_from_slice(page_data.as_ref());
    }

    let materialized = snap.materialize();
    assert_eq!(materialized.len(), expected.len());
    assert_eq!(materialized, expected);
    assert_eq!(
        snap.dirty_page_count(),
        dirty_flags.iter().filter(|&&d| d).count()
    );
}

#[hegel::test(test_cases = 200)]
fn clone_overlay_shares_base_and_matches_content(tc: TestCase) {
    let num_pages = tc.draw(integers::<usize>().min_value(1).max_value(64));
    let base_seed = tc.draw(integers::<u8>());
    let base = Arc::new(make_pages(num_pages, base_seed));

    let dirty_flags: Vec<bool> = tc.draw(vecs(booleans()).min_size(num_pages).max_size(num_pages));

    let mut dirty_pages = BTreeMap::new();
    for (idx, &dirty) in dirty_flags.iter().enumerate() {
        if dirty {
            let fill = tc.draw(integers::<u8>());
            dirty_pages.insert(idx, make_dirty_page(fill));
        }
    }

    let snap = SnapshotMemory::Overlay {
        base: Arc::clone(&base),
        dirty_pages,
    };
    let cloned = snap.clone();

    assert_eq!(snap.materialize(), cloned.materialize());

    if let (SnapshotMemory::Overlay { base: b1, .. }, SnapshotMemory::Overlay { base: b2, .. }) =
        (&snap, &cloned)
    {
        assert!(Arc::ptr_eq(b1, b2), "clone must share base Arc");
    }
}

#[hegel::test(test_cases = 200)]
fn from_dirty_round_trips_through_guest_memory(tc: TestCase) {
    let num_pages = tc.draw(integers::<usize>().min_value(1).max_value(32));
    let size = num_pages * PAGE_SIZE;

    let guest_mem =
        GuestMemoryMmap::from_ranges(&[(GuestAddress(0), size)]).expect("create guest memory");

    // Write recognizable per-page patterns
    let mut page_fills = Vec::with_capacity(num_pages);
    for page in 0..num_pages {
        let fill = tc.draw(integers::<u8>());
        page_fills.push(fill);
        let data = vec![fill; PAGE_SIZE];
        guest_mem
            .write_slice(&data, GuestAddress((page * PAGE_SIZE) as u64))
            .unwrap();
    }

    let base = Arc::new(vec![0u8; size]);

    // Build dirty bitmap
    let dirty_flags: Vec<bool> = tc.draw(vecs(booleans()).min_size(num_pages).max_size(num_pages));

    let mut bitmap_words = vec![0u64; num_pages.div_ceil(64)];
    for (i, &dirty) in dirty_flags.iter().enumerate() {
        if dirty {
            bitmap_words[i / 64] |= 1u64 << (i % 64);
        }
    }

    let snap = SnapshotMemory::from_dirty(&base, &bitmap_words, &guest_mem);

    if let SnapshotMemory::Overlay {
        base: snap_base,
        dirty_pages,
    } = &snap
    {
        assert!(Arc::ptr_eq(snap_base, &base));

        for (i, &dirty) in dirty_flags.iter().enumerate() {
            if dirty {
                assert!(dirty_pages.contains_key(&i), "page {} missing", i);
                assert!(
                    dirty_pages[&i].iter().all(|&b| b == page_fills[i]),
                    "page {} content mismatch",
                    i
                );
            } else {
                assert!(!dirty_pages.contains_key(&i), "page {} unexpected", i);
            }
        }
    } else {
        panic!("from_dirty should produce Overlay");
    }
}

#[hegel::test(test_cases = 200)]
fn write_to_guest_then_read_back_matches_materialize(tc: TestCase) {
    let num_pages = tc.draw(integers::<usize>().min_value(1).max_value(32));
    let size = num_pages * PAGE_SIZE;
    let base_seed = tc.draw(integers::<u8>());
    let base = make_pages(num_pages, base_seed);

    let dirty_flags: Vec<bool> = tc.draw(vecs(booleans()).min_size(num_pages).max_size(num_pages));

    let mut dirty_pages = BTreeMap::new();
    for (idx, &dirty) in dirty_flags.iter().enumerate() {
        if dirty {
            let fill = tc.draw(integers::<u8>());
            dirty_pages.insert(idx, make_dirty_page(fill));
        }
    }

    let snap = SnapshotMemory::Overlay {
        base: Arc::new(base.clone()),
        dirty_pages,
    };

    let expected = snap.materialize();

    // Write base first (overlay only writes dirty pages), then apply overlay
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

    assert_eq!(readback, expected);
}

#[hegel::test(test_cases = 200)]
fn full_and_overlay_with_all_dirty_produce_same_result(tc: TestCase) {
    let num_pages = tc.draw(integers::<usize>().min_value(1).max_value(32));
    let base_seed = tc.draw(integers::<u8>());
    let base = make_pages(num_pages, base_seed);

    // Build overlay where ALL pages are dirty with same content as a Full snapshot
    let dirty_seed = tc.draw(integers::<u8>());
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
        "full and all-dirty overlay must produce same result"
    );
}
