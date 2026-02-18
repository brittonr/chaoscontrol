//! Wire protocol for ChaosControl SDK ↔ VMM hypercall communication.
//!
//! This crate defines the shared memory layout, command IDs, and payload
//! encoding used between guest-side SDK and host-side VMM.  It is
//! `no_std`-compatible with zero dependencies.
//!
//! # Transport
//!
//! Communication uses a **shared memory page** at a fixed guest-physical
//! address plus an I/O port trigger:
//!
//! 1. Guest writes a [`HypercallPage`] to [`HYPERCALL_PAGE_ADDR`]
//! 2. Guest does `outb(SDK_PORT, 0)` to trigger processing
//! 3. Host reads the page from guest memory, dispatches the command
//! 4. Host writes result fields back to the page
//! 5. Guest reads result and continues
//!
//! The hypercall page sits in the E820 reserved gap (`0x9FC00..0x100000`)
//! so the Linux kernel will never allocate it, but it is backed by the
//! KVM memory region and identity-mapped by the guest page tables.

#![cfg_attr(not(feature = "std"), no_std)]

// ═══════════════════════════════════════════════════════════════════════
//  Addresses and ports
// ═══════════════════════════════════════════════════════════════════════

/// Guest-physical address of the SDK hypercall page (4 KB).
///
/// Located in the E820 reserved gap between low memory end (`0x9FC00`)
/// and HIMEM_START (`0x100000`).  The kernel sees this as reserved BIOS
/// memory and will not use it.
pub const HYPERCALL_PAGE_ADDR: u64 = 0x000F_E000;

/// I/O port used to trigger a hypercall.
///
/// Guest writes `outb(SDK_PORT, 0)` after filling the hypercall page.
pub const SDK_PORT: u16 = 0x0510;

/// Size of the hypercall page in bytes.
pub const HYPERCALL_PAGE_SIZE: usize = 4096;

/// Offset of the payload area within the hypercall page.
pub const PAYLOAD_OFFSET: usize = 32;

/// Maximum payload size in bytes.
pub const PAYLOAD_MAX: usize = HYPERCALL_PAGE_SIZE - PAYLOAD_OFFSET;

// ═══════════════════════════════════════════════════════════════════════
//  Coverage bitmap
// ═══════════════════════════════════════════════════════════════════════

/// Guest-physical address of the coverage bitmap (64 KB).
///
/// Located in the BIOS reserved area (0xE0000–0xEFFFF) within the E820
/// gap between low memory end and HIMEM_START.  The kernel sees this as
/// reserved BIOS memory and will not allocate it, but it is backed by
/// the KVM memory region and identity-mapped by the guest page tables.
///
/// The bitmap follows the AFL convention: 64 KB of 8-bit saturating
/// counters indexed by `(prev_location XOR cur_location) % MAP_SIZE`.
pub const COVERAGE_BITMAP_ADDR: u64 = 0x000E_0000;

/// Size of the coverage bitmap in bytes (64 KB, same as AFL).
pub const COVERAGE_BITMAP_SIZE: usize = 65536;

/// I/O port used to signal coverage initialization.
///
/// Guest writes `outb(COVERAGE_PORT, 0)` after mapping the bitmap.
/// This tells the VMM that coverage collection is active.
pub const COVERAGE_PORT: u16 = 0x0511;

// ═══════════════════════════════════════════════════════════════════════
//  Command IDs
// ═══════════════════════════════════════════════════════════════════════

/// Assertion: condition must be true every time this point is reached.
pub const CMD_ASSERT_ALWAYS: u8 = 0x01;

/// Assertion: condition must be true at least once across all runs.
pub const CMD_ASSERT_SOMETIMES: u8 = 0x02;

/// Assertion: this point must be reached at least once across all runs.
pub const CMD_ASSERT_REACHABLE: u8 = 0x03;

/// Assertion: this point must never be reached in any run.
pub const CMD_ASSERT_UNREACHABLE: u8 = 0x04;

/// Lifecycle: workload setup is complete, testing begins.
pub const CMD_LIFECYCLE_SETUP_COMPLETE: u8 = 0x10;

/// Lifecycle: emit a named structured event.
pub const CMD_LIFECYCLE_SEND_EVENT: u8 = 0x11;

/// Random: request a guided random u64 from the VMM.
pub const CMD_RANDOM_GET: u8 = 0x20;

/// Random: request a guided choice from `n` options (0..n-1).
pub const CMD_RANDOM_CHOICE: u8 = 0x21;

/// Coverage: signal that guest has initialized coverage instrumentation.
pub const CMD_COVERAGE_INIT: u8 = 0x30;

// ═══════════════════════════════════════════════════════════════════════
//  Status codes
// ═══════════════════════════════════════════════════════════════════════

/// Hypercall completed successfully.
pub const STATUS_OK: u8 = 0x00;

/// Hypercall failed (unknown command, bad payload, etc.).
pub const STATUS_ERROR: u8 = 0x01;

/// An `assert_always` fired with condition=false — test fails.
pub const STATUS_ASSERTION_FAILED: u8 = 0x02;

/// An `assert_unreachable` was reached — test fails.
pub const STATUS_UNREACHABLE_REACHED: u8 = 0x03;

// ═══════════════════════════════════════════════════════════════════════
//  Hypercall page layout
// ═══════════════════════════════════════════════════════════════════════

/// Fixed-layout hypercall page shared between guest SDK and host VMM.
///
/// The guest writes request fields and payload, then triggers with
/// `outb`.  The host reads the request, processes it, and writes the
/// `result` and `status` fields before the guest resumes.
///
/// Total size: 4096 bytes (one page).
///
/// ```text
/// Offset  Size  Field
/// ──────  ────  ─────────────
/// 0x00    1     command
/// 0x01    1     flags
/// 0x02    2     (reserved)
/// 0x04    4     id
/// 0x08    2     payload_len
/// 0x0A    6     (reserved)
/// 0x10    8     result        ← written by host
/// 0x18    1     status        ← written by host
/// 0x19    7     (reserved)
/// 0x20    4064  payload       ← assertion message + details
/// ```
#[repr(C, align(4096))]
#[derive(Clone)]
pub struct HypercallPage {
    // ── Request (written by guest) ──────────────────────────────
    /// Command ID (one of `CMD_*` constants).
    pub command: u8,
    /// Flags byte.  Bit 0 = condition value for assertions.
    pub flags: u8,
    pub _reserved0: [u8; 2],
    /// Caller-assigned assertion/event ID for deduplication.
    ///
    /// For assertions, this identifies the specific assertion site so
    /// the oracle can track "was this always true?" across invocations.
    /// Typically derived from a hash of the source location.
    pub id: u32,
    /// Length of the payload in bytes.
    pub payload_len: u16,
    pub _reserved1: [u8; 6],

    // ── Response (written by host) ──────────────────────────────
    /// Result value.  For `CMD_RANDOM_GET`: the random u64.
    /// For `CMD_RANDOM_CHOICE`: the chosen index.
    pub result: u64,
    /// Status code (one of `STATUS_*` constants).
    pub status: u8,
    pub _reserved2: [u8; 7],

    // ── Payload ─────────────────────────────────────────────────
    /// Variable-length payload (message + details).
    pub payload: [u8; PAYLOAD_MAX],
}

// Compile-time size check.
const _: () = assert!(core::mem::size_of::<HypercallPage>() == HYPERCALL_PAGE_SIZE);

impl HypercallPage {
    /// Create a zeroed hypercall page.
    pub const fn zeroed() -> Self {
        Self {
            command: 0,
            flags: 0,
            _reserved0: [0; 2],
            id: 0,
            payload_len: 0,
            _reserved1: [0; 6],
            result: 0,
            status: 0,
            _reserved2: [0; 7],
            payload: [0; PAYLOAD_MAX],
        }
    }

    /// Get the condition flag (bit 0 of flags).
    pub const fn condition(&self) -> bool {
        self.flags & 0x01 != 0
    }

    /// Set the condition flag.
    pub fn set_condition(&mut self, cond: bool) {
        if cond {
            self.flags |= 0x01;
        } else {
            self.flags &= !0x01;
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Payload encoding / decoding
// ═══════════════════════════════════════════════════════════════════════

/// Encode a message string and key-value details into a payload buffer.
///
/// Returns the number of bytes written, or `None` if the buffer is too small.
///
/// # Wire format
///
/// ```text
/// [u16 message_len] [message bytes]
/// [u16 num_details]
/// for each detail:
///   [u16 key_len] [key bytes] [u16 value_len] [value bytes]
/// ```
pub fn encode_payload(
    buf: &mut [u8],
    message: &str,
    details: &[(&str, &str)],
) -> Option<usize> {
    let mut offset = 0;

    // Message length + bytes
    let msg_bytes = message.as_bytes();
    let msg_len = msg_bytes.len();
    if msg_len > u16::MAX as usize {
        return None;
    }
    if offset + 2 + msg_len > buf.len() {
        return None;
    }
    buf[offset..offset + 2].copy_from_slice(&(msg_len as u16).to_le_bytes());
    offset += 2;
    buf[offset..offset + msg_len].copy_from_slice(msg_bytes);
    offset += msg_len;

    // Number of details
    let num_details = details.len();
    if num_details > u16::MAX as usize {
        return None;
    }
    if offset + 2 > buf.len() {
        return None;
    }
    buf[offset..offset + 2].copy_from_slice(&(num_details as u16).to_le_bytes());
    offset += 2;

    // Each detail: key_len, key, value_len, value
    for &(key, value) in details {
        let key_bytes = key.as_bytes();
        let val_bytes = value.as_bytes();
        let klen = key_bytes.len();
        let vlen = val_bytes.len();
        if klen > u16::MAX as usize || vlen > u16::MAX as usize {
            return None;
        }
        if offset + 2 + klen + 2 + vlen > buf.len() {
            return None;
        }
        buf[offset..offset + 2].copy_from_slice(&(klen as u16).to_le_bytes());
        offset += 2;
        buf[offset..offset + klen].copy_from_slice(key_bytes);
        offset += klen;
        buf[offset..offset + 2].copy_from_slice(&(vlen as u16).to_le_bytes());
        offset += 2;
        buf[offset..offset + vlen].copy_from_slice(val_bytes);
        offset += vlen;
    }

    Some(offset)
}

/// Decoded payload: message string and key-value detail pairs.
///
/// Only available with the `std` feature (requires heap allocation).
#[cfg(feature = "std")]
extern crate alloc;

#[cfg(feature = "std")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedPayload {
    pub message: alloc::string::String,
    pub details: alloc::vec::Vec<(alloc::string::String, alloc::string::String)>,
}

/// Decode a payload buffer into a message and details.
///
/// Only available with the `std` feature.
#[cfg(feature = "std")]
pub fn decode_payload(buf: &[u8]) -> Option<DecodedPayload> {
    use alloc::string::String;
    let mut offset = 0;

    if offset + 2 > buf.len() {
        return None;
    }
    let msg_len = u16::from_le_bytes([buf[offset], buf[offset + 1]]) as usize;
    offset += 2;

    if offset + msg_len > buf.len() {
        return None;
    }
    let message = String::from_utf8_lossy(&buf[offset..offset + msg_len]).into_owned();
    offset += msg_len;

    if offset + 2 > buf.len() {
        return None;
    }
    let num_details = u16::from_le_bytes([buf[offset], buf[offset + 1]]) as usize;
    offset += 2;

    let mut details = alloc::vec::Vec::with_capacity(num_details);
    for _ in 0..num_details {
        if offset + 2 > buf.len() {
            return None;
        }
        let klen = u16::from_le_bytes([buf[offset], buf[offset + 1]]) as usize;
        offset += 2;
        if offset + klen > buf.len() {
            return None;
        }
        let key = String::from_utf8_lossy(&buf[offset..offset + klen]).into_owned();
        offset += klen;

        if offset + 2 > buf.len() {
            return None;
        }
        let vlen = u16::from_le_bytes([buf[offset], buf[offset + 1]]) as usize;
        offset += 2;
        if offset + vlen > buf.len() {
            return None;
        }
        let value = String::from_utf8_lossy(&buf[offset..offset + vlen]).into_owned();
        offset += vlen;

        details.push((key, value));
    }

    Some(DecodedPayload { message, details })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hypercall_page_is_one_page() {
        assert_eq!(core::mem::size_of::<HypercallPage>(), 4096);
    }

    #[test]
    fn zeroed_page_is_all_zeros() {
        let page = HypercallPage::zeroed();
        assert_eq!(page.command, 0);
        assert_eq!(page.flags, 0);
        assert_eq!(page.id, 0);
        assert_eq!(page.payload_len, 0);
        assert_eq!(page.result, 0);
        assert_eq!(page.status, 0);
    }

    #[test]
    fn condition_flag_roundtrip() {
        let mut page = HypercallPage::zeroed();
        assert!(!page.condition());

        page.set_condition(true);
        assert!(page.condition());
        assert_eq!(page.flags & 0x01, 1);

        page.set_condition(false);
        assert!(!page.condition());
        assert_eq!(page.flags & 0x01, 0);
    }

    #[test]
    fn condition_flag_preserves_other_bits() {
        let mut page = HypercallPage::zeroed();
        page.flags = 0xFE; // all bits except bit 0
        page.set_condition(true);
        assert_eq!(page.flags, 0xFF);
        page.set_condition(false);
        assert_eq!(page.flags, 0xFE);
    }

    #[test]
    fn encode_simple_message() {
        let mut buf = [0u8; 256];
        let len = encode_payload(&mut buf, "hello", &[]).unwrap();
        // 2 bytes msg_len + 5 bytes "hello" + 2 bytes num_details(0)
        assert_eq!(len, 9);
        assert_eq!(&buf[2..7], b"hello");
    }

    #[test]
    fn encode_with_details() {
        let mut buf = [0u8; 256];
        let details = [("key1", "val1"), ("key2", "val2")];
        let len = encode_payload(&mut buf, "msg", &details).unwrap();
        // 2 + 3 + 2 + (2+4+2+4) + (2+4+2+4) = 29
        assert_eq!(len, 31);
    }

    #[test]
    fn encode_empty_message_and_details() {
        let mut buf = [0u8; 256];
        let len = encode_payload(&mut buf, "", &[]).unwrap();
        assert_eq!(len, 4); // 2 + 0 + 2
    }

    #[test]
    fn encode_buffer_too_small() {
        let mut buf = [0u8; 4];
        assert!(encode_payload(&mut buf, "hello world", &[]).is_none());
    }

    #[cfg(feature = "std")]
    #[test]
    fn encode_decode_roundtrip() {
        let mut buf = [0u8; 1024];
        let details = [("host", "vm-1"), ("component", "raft")];
        let len = encode_payload(&mut buf, "leader elected", &details).unwrap();

        let decoded = decode_payload(&buf[..len]).unwrap();
        assert_eq!(decoded.message, "leader elected");
        assert_eq!(decoded.details.len(), 2);
        assert_eq!(decoded.details[0], ("host".into(), "vm-1".into()));
        assert_eq!(decoded.details[1], ("component".into(), "raft".into()));
    }

    #[cfg(feature = "std")]
    #[test]
    fn decode_truncated_message() {
        let buf = [0x05, 0x00]; // msg_len=5 but no bytes follow
        assert!(decode_payload(&buf).is_none());
    }

    #[cfg(feature = "std")]
    #[test]
    fn decode_empty_payload() {
        assert!(decode_payload(&[]).is_none());
    }

    #[test]
    fn sdk_port_does_not_conflict_with_serial_or_pit() {
        // Serial: 0x3F8..=0x3FF
        assert!(SDK_PORT < 0x3F8 || SDK_PORT > 0x3FF);
        // PIT: 0x40..=0x43, 0x61
        assert!(SDK_PORT != 0x40 && SDK_PORT != 0x41 && SDK_PORT != 0x42);
        assert!(SDK_PORT != 0x43 && SDK_PORT != 0x61);
    }

    #[test]
    fn hypercall_page_addr_in_e820_gap() {
        // Must be >= LOW_MEMORY_END (0x9FC00) and < HIMEM_START (0x100000)
        assert!(HYPERCALL_PAGE_ADDR >= 0x9FC00);
        assert!(HYPERCALL_PAGE_ADDR + HYPERCALL_PAGE_SIZE as u64 <= 0x100000);
    }

    #[test]
    fn coverage_bitmap_in_e820_gap() {
        // Must be >= LOW_MEMORY_END and within reserved BIOS area
        assert!(COVERAGE_BITMAP_ADDR >= 0xA0000); // Video RAM starts here
        assert!(COVERAGE_BITMAP_ADDR + COVERAGE_BITMAP_SIZE as u64 <= 0x100000);
    }

    #[test]
    fn coverage_bitmap_does_not_overlap_hypercall_page() {
        let cov_end = COVERAGE_BITMAP_ADDR + COVERAGE_BITMAP_SIZE as u64;
        // Coverage: 0xE0000..0xF0000, Hypercall: 0xFE000..0xFF000
        assert!(cov_end <= HYPERCALL_PAGE_ADDR || COVERAGE_BITMAP_ADDR >= HYPERCALL_PAGE_ADDR + HYPERCALL_PAGE_SIZE as u64);
    }

    #[test]
    fn coverage_port_does_not_conflict() {
        assert_ne!(COVERAGE_PORT, SDK_PORT);
        assert!(COVERAGE_PORT < 0x3F8 || COVERAGE_PORT > 0x3FF); // Not serial
        assert!(COVERAGE_PORT != 0x40 && COVERAGE_PORT != 0x43); // Not PIT
    }
}
