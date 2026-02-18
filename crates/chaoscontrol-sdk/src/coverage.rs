//! Guest-side coverage instrumentation for ChaosControl.
//!
//! Provides AFL-style edge coverage collection via a shared memory bitmap
//! at a fixed guest physical address. The VMM reads this bitmap after each
//! execution quantum to guide coverage-based exploration.
//!
//! # Usage
//!
//! ## Manual coverage
//!
//! ```rust,ignore
//! use chaoscontrol_sdk::coverage;
//!
//! // Initialize coverage (must be called before any recording)
//! coverage::init();
//!
//! // Record a branch edge (typically at interesting decision points)
//! coverage::record_edge(0x1234, 0x5678);
//! ```
//!
//! ## Automatic instrumentation (SanCov)
//!
//! Compile your guest program with:
//! ```sh
//! RUSTFLAGS="-C instrument-coverage" cargo build
//! ```
//!
//! Or for C/C++ with clang:
//! ```sh
//! clang -fsanitize-coverage=trace-pc-guard ...
//! ```
//!
//! The SDK provides `__sanitizer_cov_trace_pc_guard` and
//! `__sanitizer_cov_trace_pc_guard_init` that automatically record
//! edges to the shared bitmap.
//!
//! # Transport
//!
//! The coverage bitmap occupies 64 KB at guest physical address
//! [`COVERAGE_BITMAP_ADDR`](chaoscontrol_protocol::COVERAGE_BITMAP_ADDR).
//! This is in the BIOS reserved area (0xE0000–0xEFFFF) and is
//! identity-mapped by the guest page tables.
//!
//! - `no_std`: Direct pointer access to the physical address.
//! - `std`: Maps the physical address via `/dev/mem`.

use chaoscontrol_protocol::{COVERAGE_BITMAP_ADDR, COVERAGE_BITMAP_SIZE};

// ═══════════════════════════════════════════════════════════════════════
//  State
// ═══════════════════════════════════════════════════════════════════════

/// Previous location for AFL-style edge hashing.
/// Uses a simple static — single-vCPU guests are single-threaded.
static mut PREV_LOCATION: usize = 0;

/// Whether coverage has been initialized.
static mut INITIALIZED: bool = false;

// ═══════════════════════════════════════════════════════════════════════
//  no_std transport
// ═══════════════════════════════════════════════════════════════════════

#[cfg(not(feature = "std"))]
unsafe fn bitmap_ptr() -> *mut u8 {
    COVERAGE_BITMAP_ADDR as *mut u8
}

// ═══════════════════════════════════════════════════════════════════════
//  std transport (mmap /dev/mem)
// ═══════════════════════════════════════════════════════════════════════

#[cfg(feature = "std")]
use std::sync::OnceLock;

#[cfg(feature = "std")]
static BITMAP_MAPPING: OnceLock<BitmapMapping> = OnceLock::new();

#[cfg(feature = "std")]
struct BitmapMapping {
    ptr: *mut u8,
}

#[cfg(feature = "std")]
unsafe impl Send for BitmapMapping {}
#[cfg(feature = "std")]
unsafe impl Sync for BitmapMapping {}

#[cfg(feature = "std")]
fn init_mapping() -> BitmapMapping {
    use std::fs::OpenOptions;
    use std::os::unix::io::AsRawFd;

    let fd = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/mem")
        .expect("chaoscontrol-sdk: failed to open /dev/mem for coverage bitmap");

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
        panic!("chaoscontrol-sdk: mmap /dev/mem for coverage bitmap failed");
    }

    BitmapMapping {
        ptr: ptr as *mut u8,
    }
}

#[cfg(feature = "std")]
unsafe fn bitmap_ptr() -> *mut u8 {
    BITMAP_MAPPING.get_or_init(init_mapping).ptr
}

// ═══════════════════════════════════════════════════════════════════════
//  Public API
// ═══════════════════════════════════════════════════════════════════════

/// Initialize coverage collection.
///
/// For `no_std`: no-op (bitmap is identity-mapped).
/// For `std`: maps the coverage bitmap via `/dev/mem`.
///
/// Must be called before any coverage recording. Calling multiple
/// times is safe (idempotent).
pub fn init() {
    unsafe {
        if INITIALIZED {
            return;
        }
        // Touch the bitmap pointer to trigger mmap (std) or validate (no_std)
        let _ = bitmap_ptr();
        INITIALIZED = true;
    }
}

/// Record an edge hit using AFL-style hashing.
///
/// `cur_location` identifies the current basic block / branch target.
/// The edge is hashed as `prev_location XOR cur_location` and the
/// corresponding bitmap counter is incremented (saturating at 255).
///
/// After recording, `prev_location` is updated to `cur_location >> 1`
/// (the right-shift prevents A→B and B→A from hashing to the same slot).
///
/// # Safety
///
/// [`init()`] must have been called first. In `no_std` mode, the guest
/// must have identity mapping active for the bitmap address.
#[inline(always)]
pub fn record_edge(cur_location: usize) {
    unsafe {
        let bitmap = bitmap_ptr();
        let index = (PREV_LOCATION ^ cur_location) % COVERAGE_BITMAP_SIZE;
        let counter = bitmap.add(index);
        let val = counter.read_volatile();
        counter.write_volatile(val.saturating_add(1));
        PREV_LOCATION = cur_location >> 1;
    }
}

/// Record a hit at a specific bitmap index.
///
/// Unlike [`record_edge`], this does NOT update `prev_location`.
/// Use this for manual annotation of interesting program points.
#[inline(always)]
pub fn record_hit(index: usize) {
    unsafe {
        let bitmap = bitmap_ptr();
        let idx = index % COVERAGE_BITMAP_SIZE;
        let counter = bitmap.add(idx);
        let val = counter.read_volatile();
        counter.write_volatile(val.saturating_add(1));
    }
}

/// Reset the edge tracking state (prev_location = 0).
///
/// Call this at the start of each test iteration if reusing
/// the same process across multiple test runs.
pub fn reset_state() {
    unsafe {
        PREV_LOCATION = 0;
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  SanCov-compatible hooks
// ═══════════════════════════════════════════════════════════════════════
//
// These functions are called by code compiled with:
//   -fsanitize-coverage=trace-pc-guard (clang)
//   -C instrument-coverage (rustc, via LLVM)
//
// The compiler inserts a call to __sanitizer_cov_trace_pc_guard at each
// edge, passing a pointer to a unique guard variable. The guard value
// serves as the edge identifier.

/// Called by SanCov-instrumented code at each edge.
///
/// `guard` points to a unique u32 per edge, assigned by
/// `__sanitizer_cov_trace_pc_guard_init`. We use its value as the
/// edge identifier for AFL-style hashing.
///
/// # Safety
///
/// Must be called with a valid pointer to a guard variable.
/// This is guaranteed by the compiler's SanCov instrumentation.
#[no_mangle]
pub unsafe extern "C" fn __sanitizer_cov_trace_pc_guard(guard: *mut u32) {
    let guard_val = guard.read_volatile();
    if guard_val == 0 {
        return; // Guard disabled
    }
    record_edge(guard_val as usize);
}

/// Called once at startup for each DSO with SanCov instrumentation.
///
/// Assigns sequential IDs to each guard variable in the range
/// `[start, stop)`. Guard 0 is reserved (disabled), so IDs start at 1.
///
/// # Safety
///
/// Must be called with valid pointers to guard variables.
/// This is guaranteed by the compiler's SanCov instrumentation.
#[no_mangle]
pub unsafe extern "C" fn __sanitizer_cov_trace_pc_guard_init(start: *mut u32, stop: *mut u32) {
    // Initialize coverage on first call
    if !INITIALIZED {
        init();
    }

    // Assign sequential IDs to guards
    let count = stop.offset_from(start) as usize;
    let mut current = start;
    for i in 0..count {
        if current.read_volatile() == 0 {
            // Assign ID (start at 1, 0 means disabled)
            current.write_volatile((i + 1) as u32);
        }
        current = current.add(1);
    }
}
