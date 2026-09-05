//! Bounded x86_64 fixture. The guest copies a frame, emits OUT, and increments a counter.
//! The controller stops inside NOP padding before the terminal HLT instruction.

use chaoscontrol_protocol as wire;
use chaoscontrol_vmm::memory;

pub const FRAME_ADDRESS: u64 = 0x0020_0000;
pub const COUNTER_ADDRESS: u64 = 0x0020_2000;
const ELF_HEADER_BYTES: usize = 64;
const PROGRAM_HEADER_BYTES: usize = 56;
const ELF_IDENT: &[u8] = b"\x7fELF\x02\x01\x01\0\0\0\0\0\0\0\0\0";
const EXECUTABLE_TYPE: u16 = 2;
const X86_64_MACHINE: u16 = 62;
const READ_EXECUTE_FLAGS: u32 = 5;
const PROGRAM_CAPACITY: usize = 64;
const TAIL_PADDING_BYTES: usize = PROGRAM_CAPACITY;
const NOP: u8 = 0x90;
const WORD_BYTES: usize = std::mem::size_of::<u64>();
const FRAME_WORDS: usize = wire::HYPERCALL_PAGE_SIZE / WORD_BYTES;
// KVM can single-step each REP word plus every scalar instruction and I/O exit.
pub const MAXIMUM_EXITS: u64 = FRAME_WORDS as u64 + PROGRAM_CAPACITY as u64;
const MOV_ESI: u8 = 0xbe;
const MOV_EDI: u8 = 0xbf;
const MOV_ECX: u8 = 0xb9;
const MOV_EAX: u8 = 0xb8;
const CLEAR_DIRECTION: u8 = 0xfc;
const COPY_WORDS: &[u8] = &[0xf3, 0x48, 0xa5];
const MOV_DX: &[u8] = &[0x66, 0xba];
const CLEAR_EAX: &[u8] = &[0x31, 0xc0];
const OUT_BYTE: u8 = 0xee;
const INCREMENT_QWORD: &[u8] = &[0x48, 0xff, 0x00];
const HALT: u8 = 0xf4;

pub fn image() -> Vec<u8> {
    let code = program();
    let code_offset = ELF_HEADER_BYTES + PROGRAM_HEADER_BYTES;
    let mut image = Vec::with_capacity(code_offset + code.len());
    image.extend_from_slice(ELF_IDENT);
    image.extend_from_slice(&EXECUTABLE_TYPE.to_le_bytes());
    image.extend_from_slice(&X86_64_MACHINE.to_le_bytes());
    image.extend_from_slice(&1_u32.to_le_bytes()); // ELF version.
    image.extend_from_slice(&memory::HIMEM_START.to_le_bytes());
    image.extend_from_slice(&u64::try_from(ELF_HEADER_BYTES).unwrap().to_le_bytes());
    image.extend_from_slice(&0_u64.to_le_bytes()); // No section table.
    image.extend_from_slice(&0_u32.to_le_bytes()); // No architecture flags.
    image.extend_from_slice(&u16::try_from(ELF_HEADER_BYTES).unwrap().to_le_bytes());
    image.extend_from_slice(&u16::try_from(PROGRAM_HEADER_BYTES).unwrap().to_le_bytes());
    image.extend_from_slice(&1_u16.to_le_bytes()); // One load segment.
    image.extend_from_slice(&0_u16.to_le_bytes()); // No section entries.
    image.extend_from_slice(&0_u16.to_le_bytes());
    image.extend_from_slice(&0_u16.to_le_bytes());
    assert_eq!(image.len(), ELF_HEADER_BYTES);
    image.extend_from_slice(&1_u32.to_le_bytes()); // PT_LOAD.
    image.extend_from_slice(&READ_EXECUTE_FLAGS.to_le_bytes());
    image.extend_from_slice(&u64::try_from(code_offset).unwrap().to_le_bytes());
    image.extend_from_slice(&memory::HIMEM_START.to_le_bytes());
    image.extend_from_slice(&memory::HIMEM_START.to_le_bytes());
    image.extend_from_slice(&u64::try_from(code.len()).unwrap().to_le_bytes());
    image.extend_from_slice(&u64::try_from(code.len()).unwrap().to_le_bytes());
    image.extend_from_slice(&1_u64.to_le_bytes()); // Byte-aligned segment.
    assert_eq!(image.len(), code_offset);
    image.extend_from_slice(&code);
    image
}

pub fn stop_range() -> std::ops::Range<u64> {
    let length = program().len();
    let end = memory::HIMEM_START + u64::try_from(length - 1).unwrap();
    let start = end - u64::try_from(TAIL_PADDING_BYTES).unwrap();
    start..end
}

fn program() -> Vec<u8> {
    let mut code = Vec::with_capacity(PROGRAM_CAPACITY + TAIL_PADDING_BYTES);
    immediate(&mut code, MOV_ESI, FRAME_ADDRESS);
    immediate(&mut code, MOV_EDI, wire::HYPERCALL_PAGE_ADDR);
    assert_eq!(wire::HYPERCALL_PAGE_SIZE % WORD_BYTES, 0);
    immediate(&mut code, MOV_ECX, FRAME_WORDS as u64);
    code.push(CLEAR_DIRECTION);
    code.extend_from_slice(COPY_WORDS);
    code.extend_from_slice(MOV_DX);
    code.extend_from_slice(&wire::SDK_PORT.to_le_bytes());
    code.extend_from_slice(CLEAR_EAX);
    code.push(OUT_BYTE);
    immediate(&mut code, MOV_EAX, COUNTER_ADDRESS);
    code.extend_from_slice(INCREMENT_QWORD);
    assert!(code.len() < PROGRAM_CAPACITY);
    // The bounded slice finishes after the counter but before HLT can wait for an interrupt.
    code.extend_from_slice(&[NOP; TAIL_PADDING_BYTES]);
    code.push(HALT);
    assert!(code.len() <= PROGRAM_CAPACITY + TAIL_PADDING_BYTES);
    code
}

fn immediate(code: &mut Vec<u8>, opcode: u8, value: u64) {
    code.push(opcode);
    code.extend_from_slice(&u32::try_from(value).unwrap().to_le_bytes());
}
