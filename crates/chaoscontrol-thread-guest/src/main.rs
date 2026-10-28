#![no_std]
#![no_main]

use chaoscontrol_thread_runtime::run;
use core::arch::asm;
use core::panic::PanicInfo;

static mut SHARED: u64 = 0;

fn outb(port: u16, value: u8) {
    // SAFETY: port I/O is the admitted guest serial path.
    unsafe { asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack)) };
}

fn inb(port: u16) -> u8 {
    let value: u8;
    // SAFETY: port I/O is the admitted guest serial path.
    unsafe { asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack)) };
    value
}

fn init_serial() {
    outb(0x3f9, 0x00);
    outb(0x3fb, 0x80);
    outb(0x3f8, 0x01);
    outb(0x3f9, 0x00);
    outb(0x3fb, 0x03);
    outb(0x3fa, 0xc7);
    outb(0x3fc, 0x03);
}

fn write_byte(byte: u8) {
    while inb(0x3fd) & 0x20 == 0 {}
    outb(0x3f8, byte);
}

const HEX: [u8; 16] = *b"0123456789abcdef";

fn write_hex(value: u64) {
    write_byte(HEX[((value >> 4) & 0xf) as usize]);
    write_byte(HEX[(value & 0xf) as usize]);
}

#[no_mangle]
pub extern "C" fn thread_start() -> ! {
    init_serial();
    // SAFETY: this is the only task and no other code observes SHARED yet.
    let shared = unsafe { &mut *core::ptr::addr_of_mut!(SHARED) };
    let (_ticks, paused) = unsafe { run(shared) };
    write_byte(b'R');
    write_hex(*shared);
    write_byte(b' ');
    write_byte(b'P');
    write_byte(if paused > 0 { b'1' } else { b'0' });
    write_byte(b'\n');
    // SAFETY: an empty IDT makes UD2 triple-fault and stop the VM.
    unsafe { asm!("ud2", options(noreturn)) };
}

#[no_mangle]
pub extern "C" fn main() -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn __libc_start_main() -> ! {
    loop {
        // SAFETY: this stub is never entered by the admitted guest.
        unsafe { asm!("hlt", options(nomem, nostack)) };
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    // SAFETY: an empty IDT makes UD2 triple-fault and stop the VM.
    unsafe { asm!("ud2", options(noreturn)) };
}
