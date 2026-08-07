const RESERVED_ZERO_BYTES: usize = 2;
const RESERVED_ONE_BYTES: usize = 6;
const RESERVED_TWO_BYTES: usize = 7;
const CONDITION_FLAG_MASK: u8 = 0x01;

/// Fixed-layout hypercall page shared between guest SDK and host VMM.
///
/// The guest writes request fields and payload, then triggers with
/// `outb`. The host writes the result and status before the guest resumes.
#[repr(C, align(4096))]
#[derive(Clone)]
pub struct HypercallPage {
    /// Command ID.
    pub command: u8,
    /// Flags byte. Bit 0 is the assertion condition value.
    pub flags: u8,
    pub _reserved0: [u8; RESERVED_ZERO_BYTES],
    /// Non-authoritative compact assertion or event alias.
    pub id: u32,
    /// Payload length in bytes.
    pub payload_len: u16,
    pub _reserved1: [u8; RESERVED_ONE_BYTES],
    /// Command result written by the host.
    pub result: u64,
    /// Status code written by the host.
    pub status: u8,
    pub _reserved2: [u8; RESERVED_TWO_BYTES],
    /// Variable-length payload storage.
    pub payload: [u8; crate::PAYLOAD_MAX],
}

const _: () = assert!(core::mem::size_of::<HypercallPage>() == crate::HYPERCALL_PAGE_SIZE);

impl HypercallPage {
    /// Create a zeroed hypercall page.
    pub const fn zeroed() -> Self {
        Self {
            command: 0,
            flags: 0,
            _reserved0: [0; RESERVED_ZERO_BYTES],
            id: 0,
            payload_len: 0,
            _reserved1: [0; RESERVED_ONE_BYTES],
            result: 0,
            status: 0,
            _reserved2: [0; RESERVED_TWO_BYTES],
            payload: [0; crate::PAYLOAD_MAX],
        }
    }

    /// Get the condition flag.
    pub const fn condition(&self) -> bool {
        self.flags & CONDITION_FLAG_MASK != 0
    }

    /// Set the condition flag.
    pub fn set_condition(&mut self, condition: bool) {
        if condition {
            self.flags |= CONDITION_FLAG_MASK;
        } else {
            self.flags &= !CONDITION_FLAG_MASK;
        }
    }
}
