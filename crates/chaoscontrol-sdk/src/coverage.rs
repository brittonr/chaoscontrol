//! Guest-side coverage instrumentation for ChaosControl.
//!
//! Provides AFL-style edge coverage collection via a shared memory bitmap
//! at a fixed guest physical address. The VMM reads this bitmap after each
//! execution quantum to guide coverage-based exploration.
//!
//! # No-op mode
//!
//! When built with `default-features = false`, all coverage functions
//! are no-ops. This is safe for production builds.
//!
//! # Usage
//!
//! ## Manual coverage
//!
//! ```rust,ignore
//! use chaoscontrol_sdk::coverage;
//!
//! coverage::init();
//! coverage::record_edge(0x1234);
//! ```
//!
//! ## Automatic instrumentation (SanCov)
//!
//! Compile your guest program with:
//! ```sh
//! RUSTFLAGS="-C instrument-coverage" cargo build
//! ```
//!
//! The SDK provides `__sanitizer_cov_trace_pc_guard` and
//! `__sanitizer_cov_trace_pc_guard_init` that automatically record
//! edges to the shared bitmap.

// ═══════════════════════════════════════════════════════════════════════
//  Full mode: real coverage collection
// ═══════════════════════════════════════════════════════════════════════

#[cfg(any(feature = "full", test))]
const REGION_PARTS: usize = 2;

/// Code-region end — lower half of bitmap. Matches explorer's `CODE_REGION_END`.
#[cfg(any(feature = "full", test))]
const CODE_REGION_END: usize = chaoscontrol_protocol::COVERAGE_BITMAP_SIZE / REGION_PARTS;

/// FNV-1a 64-bit hash.
#[cfg(any(feature = "full", test))]
fn fnv1a(data: &[u8]) -> u64 {
    const BASIS: u64 = 14695981039346656037;
    const PRIME: u64 = 1099511628211;
    let mut hash = BASIS;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

#[cfg(feature = "full")]
mod full {
    use chaoscontrol_protocol::{COVERAGE_BITMAP_ADDR, COVERAGE_BITMAP_SIZE};
    use std::sync::OnceLock;

    static mut PREV_LOCATION: usize = 0;
    static mut INITIALIZED: bool = false;

    static BITMAP_MAPPING: OnceLock<BitmapMapping> = OnceLock::new();

    struct BitmapMapping {
        ptr: *mut u8,
    }

    unsafe impl Send for BitmapMapping {}
    unsafe impl Sync for BitmapMapping {}

    fn init_mapping() -> BitmapMapping {
        use std::fs::OpenOptions;
        use std::os::unix::io::AsRawFd;

        let fd = match OpenOptions::new().read(true).write(true).open("/dev/mem") {
            Ok(f) => f,
            Err(_) => {
                // Outside a VM — return a heap-allocated dummy bitmap
                // so coverage calls don't crash.
                let layout =
                    std::alloc::Layout::from_size_align(COVERAGE_BITMAP_SIZE, 4096).unwrap();
                let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
                return BitmapMapping { ptr };
            }
        };

        let ptr = unsafe {
            libc::mmap(
                core::ptr::null_mut(),
                COVERAGE_BITMAP_SIZE,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd.as_raw_fd(),
                COVERAGE_BITMAP_ADDR as libc::off_t,
            )
        };
        if ptr == libc::MAP_FAILED {
            // Fallback: heap-allocated dummy bitmap
            let layout = std::alloc::Layout::from_size_align(COVERAGE_BITMAP_SIZE, 4096).unwrap();
            let fallback = unsafe { std::alloc::alloc_zeroed(layout) };
            return BitmapMapping { ptr: fallback };
        }

        BitmapMapping {
            ptr: ptr as *mut u8,
        }
    }

    unsafe fn bitmap_ptr() -> *mut u8 {
        BITMAP_MAPPING.get_or_init(init_mapping).ptr
    }

    pub fn init() {
        unsafe {
            if INITIALIZED {
                return;
            }
            let _ = bitmap_ptr();
            INITIALIZED = true;
        }
    }

    #[inline(always)]
    pub fn record_edge(cur_location: usize) {
        unsafe {
            let bitmap = bitmap_ptr();
            let index =
                (PREV_LOCATION ^ cur_location) % chaoscontrol_protocol::COVERAGE_BITMAP_SIZE;
            let counter = bitmap.add(index);
            let val = counter.read_volatile();
            counter.write_volatile(val.saturating_add(1));
            PREV_LOCATION = cur_location >> 1;
        }
    }

    #[inline(always)]
    pub fn record_hit(index: usize) {
        unsafe {
            let bitmap = bitmap_ptr();
            let idx = index % chaoscontrol_protocol::COVERAGE_BITMAP_SIZE;
            let counter = bitmap.add(idx);
            let val = counter.read_volatile();
            counter.write_volatile(val.saturating_add(1));
        }
    }

    pub fn reset_state() {
        unsafe {
            PREV_LOCATION = 0;
        }
    }

    #[inline(always)]
    pub fn record_state(pairs: &[(&str, &str)]) {
        unsafe {
            if !INITIALIZED {
                return;
            }
            let bitmap = bitmap_ptr();
            for (key, value) in pairs {
                // slot1 = fnv1a(key ++ "=" ++ value) % CODE_REGION_END
                let mut buf1 = Vec::with_capacity(key.len() + 1 + value.len());
                buf1.extend_from_slice(key.as_bytes());
                buf1.push(b'=');
                buf1.extend_from_slice(value.as_bytes());
                let slot1 = super::fnv1a(&buf1) as usize % super::CODE_REGION_END;
                let counter1 = bitmap.add(slot1);
                counter1.write_volatile(counter1.read_volatile().saturating_add(1));

                // slot2 = fnv1a(value ++ ":" ++ key) % CODE_REGION_END (reversed for diversity)
                let mut buf2 = Vec::with_capacity(value.len() + 1 + key.len());
                buf2.extend_from_slice(value.as_bytes());
                buf2.push(b':');
                buf2.extend_from_slice(key.as_bytes());
                let slot2 = super::fnv1a(&buf2) as usize % super::CODE_REGION_END;
                let counter2 = bitmap.add(slot2);
                counter2.write_volatile(counter2.read_volatile().saturating_add(1));
            }
        }
    }

    // ── SanCov hooks ────────────────────────────────────────────────

    #[no_mangle]
    pub unsafe extern "C" fn __sanitizer_cov_trace_pc_guard(guard: *mut u32) {
        let guard_val = guard.read_volatile();
        if guard_val == 0 {
            return;
        }
        record_edge(guard_val as usize);
    }

    #[no_mangle]
    pub unsafe extern "C" fn __sanitizer_cov_trace_pc_guard_init(start: *mut u32, stop: *mut u32) {
        if !INITIALIZED {
            init();
        }
        let count = stop.offset_from(start) as usize;
        let mut current = start;
        for i in 0..count {
            if current.read_volatile() == 0 {
                current.write_volatile((i + 1) as u32);
            }
            current = current.add(1);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  No-op mode: stubs when `full` is disabled
// ═══════════════════════════════════════════════════════════════════════

#[cfg(not(feature = "full"))]
mod noop {
    pub fn init() {}

    #[inline(always)]
    pub fn record_edge(_cur_location: usize) {}

    #[inline(always)]
    pub fn record_hit(_index: usize) {}

    #[inline(always)]
    pub fn record_state(_pairs: &[(&str, &str)]) {}

    pub fn reset_state() {}
}

// ═══════════════════════════════════════════════════════════════════════
//  Public API (delegates to active implementation)
// ═══════════════════════════════════════════════════════════════════════

/// Initialize coverage collection.
///
/// In full mode: maps the coverage bitmap (via `/dev/mem` in VM, or
/// a heap buffer outside).
///
/// In no-op mode: does nothing.
///
/// Safe to call multiple times (idempotent).
pub fn init() {
    #[cfg(feature = "full")]
    full::init();
    #[cfg(not(feature = "full"))]
    noop::init();
}

/// Record an edge hit using AFL-style hashing.
///
/// `cur_location` identifies the current basic block.  The edge is
/// hashed as `prev_location XOR cur_location` and the corresponding
/// bitmap counter is incremented (saturating at 255).
#[inline(always)]
pub fn record_edge(cur_location: usize) {
    #[cfg(feature = "full")]
    full::record_edge(cur_location);
    #[cfg(not(feature = "full"))]
    noop::record_edge(cur_location);
}

/// Record a hit at a specific bitmap index.
///
/// Unlike [`record_edge`], this does NOT update `prev_location`.
#[inline(always)]
pub fn record_hit(index: usize) {
    #[cfg(feature = "full")]
    full::record_hit(index);
    #[cfg(not(feature = "full"))]
    noop::record_hit(index);
}

/// Record protocol-state coverage from key-value pairs.
///
/// Hashes each `(key, value)` pair into 2 bitmap slots in the code
/// region `[0, CODE_REGION_END)` using FNV-1a. This gives the explorer
/// distinct coverage when protocol state differs (e.g. different term
/// numbers or leader counts) even when the same code paths execute.
///
/// In no-op mode: does nothing.
#[inline(always)]
pub fn record_state(pairs: &[(&str, &str)]) {
    #[cfg(feature = "full")]
    full::record_state(pairs);
    #[cfg(not(feature = "full"))]
    noop::record_state(pairs);
}

/// Reset the edge tracking state (prev_location = 0).
pub fn reset_state() {
    #[cfg(feature = "full")]
    full::reset_state();
    #[cfg(not(feature = "full"))]
    noop::reset_state();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_basic() {
        // Sanity: different inputs → different hashes
        let h1 = fnv1a(b"term=3");
        let h2 = fnv1a(b"term=5");
        assert_ne!(h1, h2);
    }

    #[test]
    fn different_values_different_slots() {
        let slot_a1 = fnv1a(b"term=3") as usize % CODE_REGION_END;
        let slot_a2 = fnv1a(b"3:term") as usize % CODE_REGION_END;
        let slot_b1 = fnv1a(b"term=5") as usize % CODE_REGION_END;
        let slot_b2 = fnv1a(b"5:term") as usize % CODE_REGION_END;

        // At least one of the two slots per pair should differ
        assert!(
            slot_a1 != slot_b1 || slot_a2 != slot_b2,
            "different values should map to different slot sets"
        );
    }

    #[test]
    fn key_domain_separation() {
        // ("term", "3") and ("index", "3") should hash differently
        let slot_term = fnv1a(b"term=3") as usize % CODE_REGION_END;
        let slot_index = fnv1a(b"index=3") as usize % CODE_REGION_END;
        assert_ne!(slot_term, slot_index, "different keys should separate");
    }

    #[test]
    fn slots_within_code_region() {
        // All slots must be in [0, CODE_REGION_END)
        for (k, v) in &[("term", "3"), ("role", "leader"), ("index", "42")] {
            let mut buf1 = std::vec::Vec::new();
            buf1.extend_from_slice(k.as_bytes());
            buf1.push(b'=');
            buf1.extend_from_slice(v.as_bytes());
            let slot1 = fnv1a(&buf1) as usize % CODE_REGION_END;
            assert!(slot1 < CODE_REGION_END);

            let mut buf2 = std::vec::Vec::new();
            buf2.extend_from_slice(v.as_bytes());
            buf2.push(b':');
            buf2.extend_from_slice(k.as_bytes());
            let slot2 = fnv1a(&buf2) as usize % CODE_REGION_END;
            assert!(slot2 < CODE_REGION_END);
        }
    }

    #[test]
    fn record_state_without_init_no_crash() {
        // Should be a no-op, no panic
        record_state(&[("term", "3"), ("role", "leader")]);
    }

    #[cfg(feature = "full")]
    #[test]
    fn record_state_after_init_no_crash() {
        init();
        record_state(&[("term", "3"), ("role", "leader")]);
    }

    #[test]
    fn two_pairs_produce_four_slots() {
        // Two pairs should touch 4 slot indices (2 per pair)
        let pairs = [("term", "3"), ("role", "leader")];
        let mut slots = std::vec::Vec::new();
        for (key, value) in &pairs {
            let mut buf1 = std::vec::Vec::new();
            buf1.extend_from_slice(key.as_bytes());
            buf1.push(b'=');
            buf1.extend_from_slice(value.as_bytes());
            slots.push(fnv1a(&buf1) as usize % CODE_REGION_END);

            let mut buf2 = std::vec::Vec::new();
            buf2.extend_from_slice(value.as_bytes());
            buf2.push(b':');
            buf2.extend_from_slice(key.as_bytes());
            slots.push(fnv1a(&buf2) as usize % CODE_REGION_END);
        }
        assert_eq!(slots.len(), 4);
    }
}
