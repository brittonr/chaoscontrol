//! Serial port constants for the ChaosControl hypervisor.
//!
//! The actual serial device is provided by [`vm_superio::Serial`] and managed
//! inside [`crate::vm::DeterministicVm`].  This module re-exports the
//! standard PC/AT I/O port addresses and IRQ numbers so that other crates
//! can reference them without depending on magic numbers.

/// Base I/O port for COM1 (standard PC/AT).
pub const COM1_BASE: u16 = 0x3F8;

/// Base I/O port for COM2.
pub const COM2_BASE: u16 = 0x2F8;

/// Base I/O port for COM3.
pub const COM3_BASE: u16 = 0x3E8;

/// Base I/O port for COM4.
pub const COM4_BASE: u16 = 0x2E8;

/// First I/O port in the COM1 register range (alias for [`COM1_BASE`]).
pub const SERIAL_PORT_BASE: u16 = COM1_BASE;

/// Last I/O port in the COM1 register range (base + 7 registers).
pub const SERIAL_PORT_END: u16 = COM1_BASE + 7;

/// IRQ line for COM1 (standard PC/AT: IRQ 4).
pub const SERIAL_IRQ: u32 = 4;

/// IRQ line for COM2 (standard PC/AT: IRQ 3).
pub const COM2_IRQ: u32 = 3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_ranges_are_sane() {
        const { assert!(SERIAL_PORT_END > SERIAL_PORT_BASE) }
        assert_eq!(SERIAL_PORT_END - SERIAL_PORT_BASE, 7);
    }

    #[test]
    fn com1_aliases() {
        assert_eq!(SERIAL_PORT_BASE, COM1_BASE);
    }

    #[test]
    fn standard_irqs() {
        assert_eq!(SERIAL_IRQ, 4);
        assert_eq!(COM2_IRQ, 3);
    }

    #[test]
    fn com_ports_distinct() {
        let ports = [COM1_BASE, COM2_BASE, COM3_BASE, COM4_BASE];
        for (i, a) in ports.iter().enumerate() {
            for (j, b) in ports.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "COM{} and COM{} must differ", i + 1, j + 1);
                }
            }
        }
    }
}
