//! Low-level transport: write to hypercall page + trigger via I/O port.
//!
//! Two implementations:
//! - `no_std` (default): Direct pointer access + inline `outb`/`inb`.
//!   Requires the guest to be in ring 0 or have IOPL=3.
//! - `std`: Accesses `/dev/mem` for the shared page and `/dev/port`
//!   for I/O port access.  Requires root or CAP_SYS_RAWIO.

use chaoscontrol_protocol::{
    HypercallPage, HYPERCALL_PAGE_ADDR, SDK_PORT,
};

// ═══════════════════════════════════════════════════════════════════════
//  no_std transport (bare-metal / ring 0)
// ═══════════════════════════════════════════════════════════════════════

/// Get a mutable pointer to the hypercall page.
///
/// # Safety
///
/// Caller must ensure:
/// - Identity mapping is active (virt == phys for this address)
/// - No concurrent access to the page
/// - We are in a ChaosControl guest (the page is backed by VMM memory)
#[cfg(not(feature = "std"))]
unsafe fn page_ptr() -> *mut HypercallPage {
    HYPERCALL_PAGE_ADDR as *mut HypercallPage
}

/// Trigger the hypercall by writing to the SDK I/O port.
///
/// # Safety
///
/// Caller must ensure the hypercall page is fully written before calling.
#[cfg(not(feature = "std"))]
unsafe fn trigger() {
    core::arch::asm!(
        "out dx, al",
        in("dx") SDK_PORT,
        in("al") 0u8,
        options(nostack, preserves_flags),
    );
}

/// Read a byte from an I/O port.
#[cfg(not(feature = "std"))]
#[allow(dead_code)]
unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    core::arch::asm!(
        "in al, dx",
        out("al") val,
        in("dx") port,
        options(nostack, preserves_flags),
    );
    val
}

// ═══════════════════════════════════════════════════════════════════════
//  std transport (Linux userspace)
// ═══════════════════════════════════════════════════════════════════════

#[cfg(feature = "std")]
use std::sync::OnceLock;

#[cfg(feature = "std")]
static PAGE: OnceLock<PageMapping> = OnceLock::new();

#[cfg(feature = "std")]
struct PageMapping {
    ptr: *mut HypercallPage,
}

// Safety: we ensure single-threaded access via the hypercall flow.
#[cfg(feature = "std")]
unsafe impl Send for PageMapping {}
#[cfg(feature = "std")]
unsafe impl Sync for PageMapping {}

#[cfg(feature = "std")]
fn init_mapping() -> PageMapping {
    use std::fs::OpenOptions;
    use std::os::unix::io::AsRawFd;

    let fd = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/mem")
        .expect("chaoscontrol-sdk: failed to open /dev/mem (need root or CAP_SYS_RAWIO)");

    let ptr = unsafe {
        libc::mmap(
            core::ptr::null_mut(),
            4096,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            fd.as_raw_fd(),
            HYPERCALL_PAGE_ADDR as libc::off_t,
        )
    };
    if ptr == libc::MAP_FAILED {
        panic!("chaoscontrol-sdk: mmap /dev/mem failed");
    }
    PageMapping {
        ptr: ptr as *mut HypercallPage,
    }
}

#[cfg(feature = "std")]
unsafe fn page_ptr() -> *mut HypercallPage {
    PAGE.get_or_init(init_mapping).ptr
}

#[cfg(feature = "std")]
unsafe fn trigger() {
    // Use /dev/port for the outb
    use std::fs::OpenOptions;
    use std::io::{Seek, SeekFrom, Write};

    let mut f = OpenOptions::new()
        .write(true)
        .open("/dev/port")
        .expect("chaoscontrol-sdk: failed to open /dev/port");
    f.seek(SeekFrom::Start(SDK_PORT as u64)).unwrap();
    f.write_all(&[0u8]).unwrap();
}

// ═══════════════════════════════════════════════════════════════════════
//  Public API (used by assert/lifecycle/random modules)
// ═══════════════════════════════════════════════════════════════════════

/// Issue a hypercall with the given command, flags, id, and payload.
///
/// Writes the request to the shared page, triggers via I/O port, and
/// returns the host-written result and status.
///
/// # Safety
///
/// This function performs raw memory access and I/O port operations.
/// It must only be called from within a ChaosControl guest VM.
pub(crate) fn hypercall(
    command: u8,
    flags: u8,
    id: u32,
    message: &str,
    details: &[(&str, &str)],
) -> (u64, u8) {
    unsafe {
        let page = &mut *page_ptr();

        // Write request fields
        page.command = command;
        page.flags = flags;
        page.id = id;
        page.result = 0;
        page.status = 0;

        // Encode payload
        let payload_len = chaoscontrol_protocol::encode_payload(
            &mut page.payload,
            message,
            details,
        )
        .unwrap_or(0);
        page.payload_len = payload_len as u16;

        // Trigger the hypercall
        trigger();

        // Read response
        (page.result, page.status)
    }
}

/// Issue a minimal hypercall (no payload) and return the result.
pub(crate) fn hypercall_simple(command: u8, id: u32) -> (u64, u8) {
    unsafe {
        let page = &mut *page_ptr();
        page.command = command;
        page.flags = 0;
        page.id = id;
        page.payload_len = 0;
        page.result = 0;
        page.status = 0;

        trigger();

        (page.result, page.status)
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Compile-time transport validation
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use chaoscontrol_protocol::PAYLOAD_MAX;

    #[test]
    fn payload_max_fits_in_page() {
        assert!(PAYLOAD_MAX > 0);
        assert!(PAYLOAD_MAX <= 4096);
    }
}
