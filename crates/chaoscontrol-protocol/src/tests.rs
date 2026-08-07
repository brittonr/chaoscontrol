const EXPECTED_HYPERCALL_PAGE_BYTES: usize = 4096;
const MESSAGE_BUFFER_BYTES: usize = 256;
const ROUNDTRIP_BUFFER_BYTES: usize = 1024;
const UNDERSIZED_BUFFER_BYTES: usize = 4;
const CONDITION_FLAG_MASK: u8 = 0x01;
const OTHER_FLAG_BITS: u8 = 0xFE;
const ALL_FLAG_BITS: u8 = 0xFF;
const SIMPLE_PAYLOAD_BYTES: usize = 11;
const JSON_PAYLOAD_BYTES: usize = 17;
const EMPTY_PAYLOAD_BYTES: usize = 6;
const MESSAGE_START: usize = 2;
const MESSAGE_END: usize = 7;
const TRUNCATED_MESSAGE_BYTES: [u8; 2] = [0x05, 0x00];
const SERIAL_PORT_START: u16 = 0x3F8;
const SERIAL_PORT_END: u16 = 0x3FF;
const PIT_CHANNEL_ZERO_PORT: u16 = 0x40;
const PIT_CHANNEL_ONE_PORT: u16 = 0x41;
const PIT_CHANNEL_TWO_PORT: u16 = 0x42;
const PIT_COMMAND_PORT: u16 = 0x43;
const PIT_SPEAKER_PORT: u16 = 0x61;
const LOW_MEMORY_END: u64 = 0x9FC00;
const VIDEO_RAM_START: u64 = 0xA0000;
const HIGH_MEMORY_START: u64 = 0x100000;
const VMCALL_BITMASK_WIDTH: u64 = 64;
const MAX_KVM_BUILTIN_VMCALL: u64 = 12;

#[test]
fn hypercall_page_is_one_page() {
    assert_eq!(
        core::mem::size_of::<crate::HypercallPage>(),
        EXPECTED_HYPERCALL_PAGE_BYTES
    );
}

#[test]
fn zeroed_page_is_all_zeros() {
    let page = crate::HypercallPage::zeroed();
    assert_eq!(page.command, 0);
    assert_eq!(page.flags, 0);
    assert_eq!(page.id, 0);
    assert_eq!(page.payload_len, 0);
    assert_eq!(page.result, 0);
    assert_eq!(page.status, 0);
}

#[test]
fn condition_flag_roundtrip() {
    let mut page = crate::HypercallPage::zeroed();
    assert!(!page.condition());
    page.set_condition(true);
    assert!(page.condition());
    assert_eq!(page.flags & CONDITION_FLAG_MASK, 1);
    page.set_condition(false);
    assert!(!page.condition());
    assert_eq!(page.flags & CONDITION_FLAG_MASK, 0);
}

#[test]
fn condition_flag_preserves_other_bits() {
    let mut page = crate::HypercallPage::zeroed();
    page.flags = OTHER_FLAG_BITS;
    page.set_condition(true);
    assert_eq!(page.flags, ALL_FLAG_BITS);
    page.set_condition(false);
    assert_eq!(page.flags, OTHER_FLAG_BITS);
}

#[test]
fn encode_simple_message_with_empty_json() {
    let mut buffer = [0_u8; MESSAGE_BUFFER_BYTES];
    let length = crate::encode_payload(&mut buffer, "hello", b"{}").unwrap();
    assert_eq!(length, SIMPLE_PAYLOAD_BYTES);
    assert_eq!(&buffer[MESSAGE_START..MESSAGE_END], b"hello");
}

#[test]
fn encode_message_with_json_details() {
    let mut buffer = [0_u8; MESSAGE_BUFFER_BYTES];
    let json = b"{\"key\":42}";
    let length = crate::encode_payload(&mut buffer, "msg", json).unwrap();
    assert_eq!(length, JSON_PAYLOAD_BYTES);
}

#[test]
fn encode_empty_message_and_empty_json() {
    let mut buffer = [0_u8; MESSAGE_BUFFER_BYTES];
    let length = crate::encode_payload(&mut buffer, "", b"{}").unwrap();
    assert_eq!(length, EMPTY_PAYLOAD_BYTES);
}

#[test]
fn encode_buffer_too_small() {
    let mut buffer = [0_u8; UNDERSIZED_BUFFER_BYTES];
    assert!(crate::encode_payload(&mut buffer, "hello world", b"{}").is_none());
}

#[cfg(feature = "std")]
#[test]
fn encode_decode_roundtrip() {
    let mut buffer = [0_u8; ROUNDTRIP_BUFFER_BYTES];
    let json = b"{\"host\":\"vm-1\",\"component\":\"raft\"}";
    let length = crate::encode_payload(&mut buffer, "leader elected", json).unwrap();
    let decoded = crate::decode_payload(&buffer[..length]).unwrap();
    assert_eq!(decoded.message, "leader elected");
    assert_eq!(decoded.json_details, json.to_vec());
}

#[cfg(feature = "std")]
#[test]
fn decode_truncated_message() {
    assert!(crate::decode_payload(&TRUNCATED_MESSAGE_BYTES).is_none());
}

#[cfg(feature = "std")]
#[test]
fn decode_empty_payload() {
    assert!(crate::decode_payload(&[]).is_none());
}

#[test]
fn sdk_port_does_not_conflict_with_serial_or_pit() {
    const { assert!(crate::SDK_PORT < SERIAL_PORT_START || crate::SDK_PORT > SERIAL_PORT_END) };
    const {
        assert!(
            crate::SDK_PORT != PIT_CHANNEL_ZERO_PORT
                && crate::SDK_PORT != PIT_CHANNEL_ONE_PORT
                && crate::SDK_PORT != PIT_CHANNEL_TWO_PORT
        )
    };
    const { assert!(crate::SDK_PORT != PIT_COMMAND_PORT && crate::SDK_PORT != PIT_SPEAKER_PORT) };
}

#[test]
fn hypercall_page_addr_in_e820_gap() {
    const { assert!(crate::HYPERCALL_PAGE_ADDR >= LOW_MEMORY_END) };
    const {
        assert!(crate::HYPERCALL_PAGE_ADDR + crate::HYPERCALL_PAGE_SIZE as u64 <= HIGH_MEMORY_START)
    };
}

#[test]
fn coverage_bitmap_in_e820_gap() {
    const { assert!(crate::COVERAGE_BITMAP_ADDR >= VIDEO_RAM_START) };
    const {
        assert!(
            crate::COVERAGE_BITMAP_ADDR + crate::COVERAGE_BITMAP_SIZE as u64 <= HIGH_MEMORY_START
        )
    };
}

#[test]
fn coverage_bitmap_does_not_overlap_hypercall_page() {
    const {
        assert!(
            crate::COVERAGE_BITMAP_ADDR + crate::COVERAGE_BITMAP_SIZE as u64
                <= crate::HYPERCALL_PAGE_ADDR
                || crate::COVERAGE_BITMAP_ADDR
                    >= crate::HYPERCALL_PAGE_ADDR + crate::HYPERCALL_PAGE_SIZE as u64
        )
    };
}

#[test]
fn coverage_port_does_not_conflict() {
    const { assert!(crate::COVERAGE_PORT != crate::SDK_PORT) };
    const { assert!(crate::COVERAGE_PORT < SERIAL_PORT_START || crate::COVERAGE_PORT > SERIAL_PORT_END) };
    const {
        assert!(
            crate::COVERAGE_PORT != PIT_CHANNEL_ZERO_PORT
                && crate::COVERAGE_PORT != PIT_COMMAND_PORT
        )
    };
}

#[test]
fn vmcall_nr_fits_in_bitmask() {
    const { assert!(crate::VMCALL_NR < VMCALL_BITMASK_WIDTH) };
}

#[test]
fn vmcall_nr_no_conflict_with_kvm_builtins() {
    const { assert!(crate::VMCALL_NR > MAX_KVM_BUILTIN_VMCALL) };
}
