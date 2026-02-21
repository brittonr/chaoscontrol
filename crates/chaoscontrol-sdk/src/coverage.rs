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

/// Reset the edge tracking state (prev_location = 0).
pub fn reset_state() {
    #[cfg(feature = "full")]
    full::reset_state();
    #[cfg(not(feature = "full"))]
    noop::reset_state();
}
