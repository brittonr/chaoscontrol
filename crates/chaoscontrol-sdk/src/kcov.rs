//! Kernel coverage (KCOV) integration for ChaosControl.
//!
//! When the guest kernel is built with `CONFIG_KCOV=y`, this module
//! enables per-task kernel code coverage collection.  Kernel PCs are
//! hashed into the same AFL-style coverage bitmap used by userspace
//! SanCov, giving the explorer visibility into kernel code paths
//! exercised by different fault schedules.
//!
//! # How it works
//!
//! 1. Mount debugfs at `/sys/kernel/debug`
//! 2. Open `/sys/kernel/debug/kcov`
//! 3. `ioctl(KCOV_INIT_TRACE)` — allocate a ring buffer of PC entries
//! 4. `mmap` the buffer into userspace
//! 5. `ioctl(KCOV_ENABLE, KCOV_TRACE_PC)` — start recording
//! 6. Periodically call [`collect()`] to drain kernel PCs into the
//!    shared coverage bitmap (AFL-style edge hashing)
//!
//! # Requirements
//!
//! - Guest kernel built with `CONFIG_KCOV=y` and
//!   `CONFIG_KCOV_INSTRUMENT_ALL=y`
//! - debugfs mountable (guest must have `/sys/kernel/debug`)
//! - Only available with the `std` feature (uses file I/O + ioctl)
//!
//! # Coverage model
//!
//! KCOV records raw PCs (one per basic block hit during the current
//! task's syscalls).  We convert these to AFL-style edges:
//!
//! ```text
//! edge_index = (prev_pc XOR cur_pc) % BITMAP_SIZE
//! bitmap[edge_index] = saturating_add(1)
//! prev_pc = cur_pc >> 1
//! ```
//!
//! This means kernel and userspace edges share the same 64 KB bitmap
//! and are naturally merged — no VMM changes needed.
//!
//! # Example
//!
//! ```rust,ignore
//! use chaoscontrol_sdk::{coverage, kcov};
//!
//! coverage::init();
//! kcov::init();  // returns false if KCOV unavailable
//!
//! loop {
//!     do_syscall_heavy_work();
//!     kcov::collect();  // drain kernel PCs → coverage bitmap
//! }
//! ```

use crate::coverage;

// ═══════════════════════════════════════════════════════════════════════
//  KCOV ioctl constants (from linux/kcov.h)
// ═══════════════════════════════════════════════════════════════════════

/// `ioctl(fd, KCOV_INIT_TRACE, cover_size)` — allocate cover_size entries.
///
/// `_IOR('c', 1, unsigned long)` = (2 << 30) | (8 << 16) | (0x63 << 8) | 1
///
/// Note: `libc::Ioctl` is `c_int` on musl and `c_ulong` on glibc.
/// The value fits in i32 (0x8008_6301 is negative in i32, but that's
/// how Linux ioctl numbers work with the direction bits).
const KCOV_INIT_TRACE: libc::Ioctl = 0x8008_6301_u32 as libc::Ioctl;

/// `ioctl(fd, KCOV_ENABLE, mode)` — start tracing.
///
/// `_IO('c', 100)` = (0x63 << 8) | 100
const KCOV_ENABLE: libc::Ioctl = 0x6364 as libc::Ioctl;

/// `ioctl(fd, KCOV_DISABLE)` — stop tracing.
///
/// `_IO('c', 101)` = (0x63 << 8) | 101
const KCOV_DISABLE: libc::Ioctl = 0x6365 as libc::Ioctl;

/// Trace mode: record program counter values.
const KCOV_TRACE_PC: libc::c_ulong = 0;

// ═══════════════════════════════════════════════════════════════════════
//  Configuration
// ═══════════════════════════════════════════════════════════════════════

/// Number of u64 entries in the KCOV ring buffer.
///
/// The first entry is the count of recorded PCs; entries 1..=count are
/// the PCs themselves.  64K entries covers deep syscall paths without
/// excessive memory use.
const COVER_SIZE: usize = 64 * 1024;

/// Path to the KCOV device.
const KCOV_PATH: &core::ffi::CStr = c"/sys/kernel/debug/kcov";

/// Path to mount debugfs.
const DEBUGFS_MOUNT: &core::ffi::CStr = c"/sys/kernel/debug";

/// Filesystem type for debugfs.
const DEBUGFS_TYPE: &core::ffi::CStr = c"debugfs";

// ═══════════════════════════════════════════════════════════════════════
//  State
// ═══════════════════════════════════════════════════════════════════════

/// Pointer to the mmaped KCOV buffer (array of u64).
static mut KCOV_AREA: *mut u64 = core::ptr::null_mut();

/// File descriptor for /sys/kernel/debug/kcov.
static mut KCOV_FD: i32 = -1;

/// Whether KCOV has been successfully initialized.
static mut INITIALIZED: bool = false;

/// Previous PC for AFL-style edge hashing (separate from userspace
/// prev_location to avoid cross-domain interference).
static mut PREV_PC: usize = 0;

/// Cumulative count of kernel PCs collected across all [`collect()`] calls.
static mut TOTAL_PCS_COLLECTED: u64 = 0;

// ═══════════════════════════════════════════════════════════════════════
//  Public API
// ═══════════════════════════════════════════════════════════════════════

/// Initialize kernel coverage collection.
///
/// Mounts debugfs, opens the KCOV device, allocates the trace buffer,
/// and enables PC tracing for the current task.
///
/// Returns `true` if KCOV was successfully enabled, `false` if the
/// kernel doesn't support it (no `CONFIG_KCOV=y`).  Calling on a
/// non-KCOV kernel is safe — it just returns `false`.
///
/// # Panics
///
/// Does not panic.  All failures return `false` with an eprintln
/// diagnostic.
pub fn init() -> bool {
    unsafe {
        if INITIALIZED {
            return true;
        }

        // ── Step 1: mount debugfs ────────────────────────────────
        if !mount_debugfs() {
            eprintln!("kcov: debugfs mount failed (KCOV unavailable)");
            return false;
        }

        // ── Step 2: open /sys/kernel/debug/kcov ──────────────────
        let fd = libc::open(KCOV_PATH.as_ptr(), libc::O_RDWR);
        if fd < 0 {
            let errno = *libc::__errno_location();
            eprintln!("kcov: open failed (errno={errno}) — kernel lacks CONFIG_KCOV?");
            return false;
        }

        // ── Step 3: allocate trace buffer ────────────────────────
        let ret = libc::ioctl(fd, KCOV_INIT_TRACE, COVER_SIZE as libc::c_ulong);
        if ret != 0 {
            let errno = *libc::__errno_location();
            eprintln!("kcov: KCOV_INIT_TRACE failed (errno={errno})");
            libc::close(fd);
            return false;
        }

        // ── Step 4: mmap the buffer ──────────────────────────────
        let buf_size = COVER_SIZE * core::mem::size_of::<u64>();
        let area = libc::mmap(
            core::ptr::null_mut(),
            buf_size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            fd,
            0,
        );
        if area == libc::MAP_FAILED {
            let errno = *libc::__errno_location();
            eprintln!("kcov: mmap failed (errno={errno})");
            libc::close(fd);
            return false;
        }

        // ── Step 5: enable tracing ───────────────────────────────
        let ret = libc::ioctl(fd, KCOV_ENABLE, KCOV_TRACE_PC);
        if ret != 0 {
            let errno = *libc::__errno_location();
            eprintln!("kcov: KCOV_ENABLE failed (errno={errno})");
            libc::munmap(area, buf_size);
            libc::close(fd);
            return false;
        }

        // ── Success ──────────────────────────────────────────────
        KCOV_AREA = area as *mut u64;
        KCOV_FD = fd;
        INITIALIZED = true;
        PREV_PC = 0;
        TOTAL_PCS_COLLECTED = 0;

        // Reset the count to zero (kernel may have written during init)
        core::ptr::write_volatile(KCOV_AREA, 0);

        true
    }
}

/// Drain kernel PCs from the KCOV buffer into the coverage bitmap.
///
/// Reads all PCs recorded since the last [`collect()`] (or [`init()`]),
/// hashes them as AFL-style edges, and writes hits to the shared
/// coverage bitmap.  Then resets the KCOV counter to zero for the
/// next collection cycle.
///
/// Returns the number of kernel PCs collected this call.  Returns 0
/// if KCOV is not initialized.
///
/// # When to call
///
/// Call this after each "interesting" guest operation — typically at
/// the end of each iteration in the main workload loop.  More frequent
/// calls give finer-grained coverage but add overhead.
pub fn collect() -> usize {
    unsafe {
        if !INITIALIZED {
            return 0;
        }

        // Read the number of PCs recorded by the kernel.
        // Entry 0 of the KCOV buffer is the count.
        let n = core::ptr::read_volatile(KCOV_AREA) as usize;
        if n == 0 {
            return 0;
        }

        // Clamp to buffer capacity (defensive).
        let n = n.min(COVER_SIZE - 1);

        // Hash kernel PCs into the coverage bitmap using AFL-style edges.
        // We maintain a separate prev_pc for kernel edges to avoid
        // interfering with userspace edge tracking.
        for i in 1..=n {
            let pc = core::ptr::read_volatile(KCOV_AREA.add(i)) as usize;
            // AFL-style: edge = prev XOR cur, then record
            let edge_index = (PREV_PC ^ pc) % chaoscontrol_protocol::COVERAGE_BITMAP_SIZE;
            coverage::record_hit(edge_index);
            PREV_PC = pc >> 1;
        }

        // Reset the KCOV counter for the next collection cycle.
        core::ptr::write_volatile(KCOV_AREA, 0);

        TOTAL_PCS_COLLECTED += n as u64;
        n
    }
}

/// Get the total number of kernel PCs collected across all calls.
pub fn total_pcs_collected() -> u64 {
    unsafe { TOTAL_PCS_COLLECTED }
}

/// Check whether KCOV is active.
pub fn is_active() -> bool {
    unsafe { INITIALIZED }
}

/// Disable KCOV tracing and release resources.
///
/// Safe to call even if KCOV was never initialized (no-op).
pub fn disable() {
    unsafe {
        if !INITIALIZED {
            return;
        }

        let _ = libc::ioctl(KCOV_FD, KCOV_DISABLE);

        let buf_size = COVER_SIZE * core::mem::size_of::<u64>();
        libc::munmap(KCOV_AREA as *mut libc::c_void, buf_size);
        libc::close(KCOV_FD);

        KCOV_AREA = core::ptr::null_mut();
        KCOV_FD = -1;
        INITIALIZED = false;
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Internal helpers
// ═══════════════════════════════════════════════════════════════════════

/// Mount debugfs at /sys/kernel/debug.  Returns true on success or if
/// already mounted (EBUSY).
unsafe fn mount_debugfs() -> bool {
    // Ensure the mount point exists
    libc::mkdir(c"/sys".as_ptr(), 0o755);
    libc::mkdir(c"/sys/kernel".as_ptr(), 0o755);
    libc::mkdir(DEBUGFS_MOUNT.as_ptr(), 0o755);

    let ret = libc::mount(
        DEBUGFS_TYPE.as_ptr().cast(),
        DEBUGFS_MOUNT.as_ptr().cast(),
        DEBUGFS_TYPE.as_ptr().cast(),
        0,
        core::ptr::null(),
    );
    if ret != 0 {
        let errno = *libc::__errno_location();
        // EBUSY = already mounted, that's fine
        if errno != libc::EBUSY {
            return false;
        }
    }
    true
}
