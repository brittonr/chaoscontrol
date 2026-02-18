//! Deterministic PIT (8254 Programmable Interval Timer) emulation.
//!
//! This module provides a TSC-based PIT implementation for deterministic replay.
//! Unlike KVM's internal PIT which runs on host wall time, this PIT counts based
//! on virtual TSC ticks that advance deterministically on each VM exit.
//!
//! The PIT has 3 channels and operates at 1,193,182 Hz. Channel 0 is typically
//! used for timer interrupts (IRQ 0), channel 1 for legacy DRAM refresh (unused),
//! and channel 2 for PC speaker and TSC calibration.

/// PIT operating frequency in Hz (1.193182 MHz)
pub const PIT_FREQ_HZ: u64 = 1_193_182;

/// I/O port for PIT channel 0 data
pub const PIT_PORT_CHANNEL0: u16 = 0x40;

/// I/O port for PIT channel 1 data
pub const PIT_PORT_CHANNEL1: u16 = 0x41;

/// I/O port for PIT channel 2 data
pub const PIT_PORT_CHANNEL2: u16 = 0x42;

/// I/O port for PIT command register
pub const PIT_PORT_COMMAND: u16 = 0x43;

/// I/O port for system control port B (gate/speaker control)
pub const PORT_SYSTEM_CONTROL_B: u16 = 0x61;

/// PIT channel operating mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelMode {
    /// Mode 0: Interrupt on terminal count
    Mode0,
    /// Mode 1: Hardware retriggerable one-shot
    Mode1,
    /// Mode 2: Rate generator (periodic interrupt)
    Mode2,
    /// Mode 3: Square wave generator
    Mode3,
    /// Mode 4: Software triggered strobe
    Mode4,
    /// Mode 5: Hardware triggered strobe
    Mode5,
}

/// PIT channel access mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessMode {
    /// Latch count command
    Latch,
    /// Access low byte only
    LoByte,
    /// Access high byte only
    HiByte,
    /// Access low byte then high byte
    LoHiByte,
}

/// Write state for LoHiByte access mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteState {
    /// Next write is low byte
    Lo,
    /// Next write is high byte
    Hi,
}

/// Read state for LoHiByte access mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadState {
    /// Next read is low byte
    Lo,
    /// Next read is high byte
    Hi,
}

/// State of a single PIT channel
#[derive(Debug, Clone)]
struct PitChannel {
    /// Reload/initial count value (0 means 65536)
    reload: u16,
    /// Operating mode
    mode: ChannelMode,
    /// Access mode
    access: AccessMode,
    /// Channel is fully programmed and counting
    armed: bool,
    /// Virtual TSC value when channel was armed
    start_tsc: u64,
    /// Write state for LoHiByte mode
    write_state: WriteState,
    /// Read state for LoHiByte mode
    read_state: ReadState,
    /// Latched count value from latch command
    latched_count: Option<u16>,
    /// Gate input (always true for ch0, controlled by port 0x61 for ch2)
    gate: bool,
}

impl PitChannel {
    /// Create a new PIT channel with default state
    fn new(channel_num: usize) -> Self {
        Self {
            reload: 0,
            mode: ChannelMode::Mode0,
            access: AccessMode::LoHiByte,
            armed: false,
            start_tsc: 0,
            write_state: WriteState::Lo,
            read_state: ReadState::Lo,
            latched_count: None,
            gate: channel_num == 0, // Channel 0 gate is always high
        }
    }

    /// Compute the current counter value based on elapsed PIT ticks
    fn compute_counter(&self, elapsed_pit_ticks: u64) -> u16 {
        crate::verified::pit::compute_counter(self.mode, self.reload, self.armed, elapsed_pit_ticks)
    }

    /// Get the output state for this channel
    fn output(&self, elapsed_pit_ticks: u64) -> bool {
        crate::verified::pit::compute_output(self.mode, self.armed, self.gate, self.reload, elapsed_pit_ticks)
    }
}

/// Snapshot of a PIT channel for save/restore
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelSnapshot {
    /// Reload/initial count value
    pub reload: u16,
    /// Operating mode
    pub mode: ChannelMode,
    /// Access mode
    pub access: AccessMode,
    /// Channel is armed
    pub armed: bool,
    /// Start TSC value
    pub start_tsc: u64,
    /// Write state
    pub write_state: WriteState,
    /// Read state
    pub read_state: ReadState,
    /// Latched count
    pub latched_count: Option<u16>,
    /// Gate input
    pub gate: bool,
}

/// Deterministic PIT implementation
///
/// # Example Usage in VMM Run Loop
///
/// ```no_run
/// # use chaoscontrol_vmm::devices::pit::{DeterministicPit, PIT_PORT_CHANNEL0};
/// # let mut pit = DeterministicPit::new(2_400_000);
/// # let mut virtual_tsc = 0u64;
/// # let port = PIT_PORT_CHANNEL0;
/// # let mut data = [0u8];
/// // Before vcpu.run():
/// if pit.pending_irq(virtual_tsc) {
///     // vm.set_irq_line(0, true)?;
///     // vm.set_irq_line(0, false)?;
///     pit.acknowledge_irq();
/// }
///
/// // On IoIn exit:
/// if DeterministicPit::handles_port(port) {
///     data[0] = pit.read_port(port, virtual_tsc);
/// }
///
/// // On IoOut exit:
/// if DeterministicPit::handles_port(port) {
///     pit.write_port(port, data[0], virtual_tsc);
/// }
/// ```
pub struct DeterministicPit {
    /// The 3 PIT channels
    channels: [PitChannel; 3],
    /// TSC frequency in KHz
    tsc_khz: u32,
    /// System control port B value (port 0x61)
    port_61: u8,
    /// Total IRQ 0 edges delivered
    irqs_delivered: u64,
}

impl DeterministicPit {
    /// Create a new deterministic PIT
    ///
    /// # Arguments
    /// * `tsc_khz` - TSC frequency in KHz (e.g., 2400000 for 2.4 GHz)
    pub fn new(tsc_khz: u32) -> Self {
        Self {
            channels: [
                PitChannel::new(0),
                PitChannel::new(1),
                PitChannel::new(2),
            ],
            tsc_khz,
            port_61: 0,
            irqs_delivered: 0,
        }
    }

    /// Check if the PIT handles the given I/O port
    pub fn handles_port(port: u16) -> bool {
        crate::verified::pit::handles_port(port)
    }

    /// Convert elapsed TSC ticks to PIT ticks
    fn tsc_to_pit_ticks(&self, elapsed_tsc: u64) -> u64 {
        crate::verified::pit::tsc_to_pit_ticks(elapsed_tsc, self.tsc_khz)
    }

    /// Get elapsed PIT ticks for a channel since it was armed
    fn elapsed_pit_ticks(&self, channel: usize, current_tsc: u64) -> u64 {
        if !self.channels[channel].armed {
            return 0;
        }
        let elapsed_tsc = current_tsc.saturating_sub(self.channels[channel].start_tsc);
        self.tsc_to_pit_ticks(elapsed_tsc)
    }

    /// Handle write to a PIT I/O port
    ///
    /// # Arguments
    /// * `port` - I/O port address
    /// * `value` - Byte value to write
    /// * `current_tsc` - Current virtual TSC value
    pub fn write_port(&mut self, port: u16, value: u8, current_tsc: u64) {
        match port {
            PIT_PORT_CHANNEL0 => self.write_channel_data(0, value, current_tsc),
            PIT_PORT_CHANNEL1 => self.write_channel_data(1, value, current_tsc),
            PIT_PORT_CHANNEL2 => self.write_channel_data(2, value, current_tsc),
            PIT_PORT_COMMAND => self.write_command(value, current_tsc),
            PORT_SYSTEM_CONTROL_B => self.write_port_61(value),
            _ => {}
        }
    }

    /// Handle read from a PIT I/O port
    ///
    /// # Arguments
    /// * `port` - I/O port address
    /// * `current_tsc` - Current virtual TSC value
    ///
    /// # Returns
    /// Byte value read from the port
    pub fn read_port(&mut self, port: u16, current_tsc: u64) -> u8 {
        match port {
            PIT_PORT_CHANNEL0 => self.read_channel_data(0, current_tsc),
            PIT_PORT_CHANNEL1 => self.read_channel_data(1, current_tsc),
            PIT_PORT_CHANNEL2 => self.read_channel_data(2, current_tsc),
            PIT_PORT_COMMAND => 0, // Command register is write-only
            PORT_SYSTEM_CONTROL_B => self.read_port_61(current_tsc),
            _ => 0,
        }
    }

    /// Check if IRQ 0 should be raised
    ///
    /// # Arguments
    /// * `current_tsc` - Current virtual TSC value
    ///
    /// # Returns
    /// `true` if an IRQ should be delivered
    pub fn pending_irq(&self, current_tsc: u64) -> bool {
        let elapsed_pit_ticks = self.elapsed_pit_ticks(0, current_tsc);
        crate::verified::pit::pending_irq_check(
            self.channels[0].mode,
            self.channels[0].armed,
            self.channels[0].reload,
            elapsed_pit_ticks,
            self.irqs_delivered,
        )
    }

    /// Check if channel N is armed (for debugging).
    pub fn channel_armed(&self, ch: usize) -> bool {
        self.channels[ch].armed
    }

    /// Get channel N reload value (for debugging).
    pub fn channel_reload(&self, ch: usize) -> u16 {
        self.channels[ch].reload
    }

    /// Get channel N mode (for debugging).
    pub fn channel_mode(&self, ch: usize) -> ChannelMode {
        self.channels[ch].mode
    }

    /// Get the number of IRQs delivered (for debugging).
    pub fn irqs_delivered(&self) -> u64 {
        self.irqs_delivered
    }

    /// Acknowledge that an IRQ was delivered
    pub fn acknowledge_irq(&mut self) {
        self.irqs_delivered += 1;
    }

    /// Compute the virtual TSC value at which the next IRQ 0 will fire.
    ///
    /// Returns `None` if channel 0 is not armed or the mode doesn't
    /// generate repeating interrupts.
    pub fn next_irq_tsc(&self) -> Option<u64> {
        crate::verified::pit::next_irq_tsc_compute(
            self.channels[0].mode,
            self.channels[0].armed,
            self.channels[0].reload,
            self.channels[0].start_tsc,
            self.tsc_khz,
            self.irqs_delivered,
        )
    }

    /// Write to command register (port 0x43)
    fn write_command(&mut self, value: u8, current_tsc: u64) {
        // Use verified function to decode command byte
        let (channel_select, access_mode, mode) = crate::verified::pit::decode_command(value);

        // Handle readback command (channel_select == 3)
        if channel_select == 3 {
            self.handle_readback(value, current_tsc);
            return;
        }

        // Handle latch command (access_mode == 0) before taking mutable ref
        if access_mode == 0 {
            let ch_idx = channel_select as usize;
            if self.channels[ch_idx].latched_count.is_none() {
                let elapsed = self.elapsed_pit_ticks(ch_idx, current_tsc);
                let count = self.channels[ch_idx].compute_counter(elapsed);
                self.channels[ch_idx].latched_count = Some(count);
                self.channels[ch_idx].read_state = ReadState::Lo;
            }
            return;
        }

        let access = match access_mode {
            1 => AccessMode::LoByte,
            2 => AccessMode::HiByte,
            3 => AccessMode::LoHiByte,
            _ => unreachable!(),
        };

        let channel = &mut self.channels[channel_select as usize];

        channel.access = access;
        channel.mode = mode;
        channel.armed = false;
        channel.write_state = WriteState::Lo;
        channel.read_state = ReadState::Lo;
        channel.latched_count = None;

        // Reset IRQ counter when channel 0 is reprogrammed
        if channel_select == 0 {
            self.irqs_delivered = 0;
        }
    }

    /// Handle readback command
    fn handle_readback(&mut self, value: u8, current_tsc: u64) {
        let latch_count = (value & 0x20) == 0;
        let _latch_status = (value & 0x10) == 0; // Status readback not implemented

        for i in 0..3 {
            if (value & (1 << (1 + i))) != 0
                && latch_count
                && self.channels[i].latched_count.is_none()
            {
                let elapsed = self.elapsed_pit_ticks(i, current_tsc);
                let count = self.channels[i].compute_counter(elapsed);
                self.channels[i].latched_count = Some(count);
                self.channels[i].read_state = ReadState::Lo;
            }
        }
    }

    /// Write data to a channel
    fn write_channel_data(&mut self, channel_num: usize, value: u8, current_tsc: u64) {
        let was_armed = self.channels[channel_num].armed;
        let channel = &mut self.channels[channel_num];

        match channel.access {
            AccessMode::Latch => {
                // Latch command doesn't accept data writes
            }
            AccessMode::LoByte => {
                channel.reload = (channel.reload & 0xFF00) | (value as u16);
                channel.start_tsc = current_tsc;
                channel.armed = true;
            }
            AccessMode::HiByte => {
                channel.reload = (channel.reload & 0x00FF) | ((value as u16) << 8);
                channel.start_tsc = current_tsc;
                channel.armed = true;
            }
            AccessMode::LoHiByte => {
                match channel.write_state {
                    WriteState::Lo => {
                        channel.reload = (channel.reload & 0xFF00) | (value as u16);
                        channel.write_state = WriteState::Hi;
                        channel.armed = false;
                    }
                    WriteState::Hi => {
                        channel.reload = (channel.reload & 0x00FF) | ((value as u16) << 8);
                        channel.write_state = WriteState::Lo;
                        channel.start_tsc = current_tsc;
                        channel.armed = true;
                    }
                }
            }
        }

        // Reset IRQ delivery counter when channel 0 is freshly armed.
        // This ensures one-shot modes (0, 1, 4, 5) can fire again after
        // being re-programmed by the kernel's clockevent framework.
        if channel_num == 0 && self.channels[0].armed && !was_armed {
            self.irqs_delivered = 0;
        }
    }

    /// Read data from a channel
    fn read_channel_data(&mut self, channel_num: usize, current_tsc: u64) -> u8 {
        // Compute count before taking mutable ref to avoid borrow conflict
        let count = if let Some(latched) = self.channels[channel_num].latched_count {
            latched
        } else {
            let elapsed = self.elapsed_pit_ticks(channel_num, current_tsc);
            self.channels[channel_num].compute_counter(elapsed)
        };

        let channel = &mut self.channels[channel_num];

        let result = match channel.access {
            AccessMode::Latch | AccessMode::LoByte => count as u8,
            AccessMode::HiByte => (count >> 8) as u8,
            AccessMode::LoHiByte => match channel.read_state {
                ReadState::Lo => {
                    channel.read_state = ReadState::Hi;
                    count as u8
                }
                ReadState::Hi => {
                    channel.read_state = ReadState::Lo;
                    // Clear latched value after reading both bytes
                    if channel.latched_count.is_some() {
                        channel.latched_count = None;
                    }
                    (count >> 8) as u8
                }
            },
        };

        // Clear latched value for single-byte access modes
        if matches!(channel.access, AccessMode::LoByte | AccessMode::HiByte) {
            channel.latched_count = None;
        }

        result
    }

    /// Write to port 0x61 (system control port B)
    fn write_port_61(&mut self, value: u8) {
        // Bit 0: Timer 2 gate
        let gate = (value & 0x01) != 0;
        self.channels[2].gate = gate;

        // Bits 0-1 are writable, store them
        self.port_61 = (self.port_61 & 0xFC) | (value & 0x03);
    }

    /// Read from port 0x61 (system control port B)
    fn read_port_61(&mut self, current_tsc: u64) -> u8 {
        let mut result = self.port_61;

        // Bit 4: Refresh cycle toggle (toggle on every read)
        result ^= 0x10;
        self.port_61 = result;

        // Bit 5: Timer 2 output
        let elapsed = self.elapsed_pit_ticks(2, current_tsc);
        let timer2_out = self.channels[2].output(elapsed);
        if timer2_out {
            result |= 0x20;
        } else {
            result &= !0x20;
        }

        result
    }

    /// Create a snapshot of the current PIT state
    pub fn snapshot(&self) -> PitSnapshot {
        PitSnapshot {
            channels: [
                self.snapshot_channel(0),
                self.snapshot_channel(1),
                self.snapshot_channel(2),
            ],
            tsc_khz: self.tsc_khz,
            port_61: self.port_61,
            irqs_delivered: self.irqs_delivered,
        }
    }

    /// Snapshot a single channel
    fn snapshot_channel(&self, channel_num: usize) -> ChannelSnapshot {
        let ch = &self.channels[channel_num];
        ChannelSnapshot {
            reload: ch.reload,
            mode: ch.mode,
            access: ch.access,
            armed: ch.armed,
            start_tsc: ch.start_tsc,
            write_state: ch.write_state,
            read_state: ch.read_state,
            latched_count: ch.latched_count,
            gate: ch.gate,
        }
    }

    /// Restore PIT state from a snapshot
    pub fn restore(snapshot: &PitSnapshot) -> Self {
        Self {
            channels: [
                Self::restore_channel(&snapshot.channels[0]),
                Self::restore_channel(&snapshot.channels[1]),
                Self::restore_channel(&snapshot.channels[2]),
            ],
            tsc_khz: snapshot.tsc_khz,
            port_61: snapshot.port_61,
            irqs_delivered: snapshot.irqs_delivered,
        }
    }

    /// Restore a single channel from snapshot
    fn restore_channel(snapshot: &ChannelSnapshot) -> PitChannel {
        PitChannel {
            reload: snapshot.reload,
            mode: snapshot.mode,
            access: snapshot.access,
            armed: snapshot.armed,
            start_tsc: snapshot.start_tsc,
            write_state: snapshot.write_state,
            read_state: snapshot.read_state,
            latched_count: snapshot.latched_count,
            gate: snapshot.gate,
        }
    }
}

/// Snapshot of PIT state for save/restore
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PitSnapshot {
    /// Channel snapshots
    pub channels: [ChannelSnapshot; 3],
    /// TSC frequency in KHz
    pub tsc_khz: u32,
    /// Port 0x61 state
    pub port_61: u8,
    /// IRQs delivered
    pub irqs_delivered: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_TSC_KHZ: u32 = 2_400_000; // 2.4 GHz

    #[test]
    fn test_new_default_state() {
        let pit = DeterministicPit::new(TEST_TSC_KHZ);
        assert_eq!(pit.tsc_khz, TEST_TSC_KHZ);
        assert_eq!(pit.irqs_delivered, 0);
        assert_eq!(pit.port_61, 0);

        // Channel 0 gate should be high
        assert!(pit.channels[0].gate);
        // Channel 2 gate should be low
        assert!(!pit.channels[2].gate);
    }

    #[test]
    fn test_handles_port() {
        assert!(DeterministicPit::handles_port(PIT_PORT_CHANNEL0));
        assert!(DeterministicPit::handles_port(PIT_PORT_CHANNEL1));
        assert!(DeterministicPit::handles_port(PIT_PORT_CHANNEL2));
        assert!(DeterministicPit::handles_port(PIT_PORT_COMMAND));
        assert!(DeterministicPit::handles_port(PORT_SYSTEM_CONTROL_B));
        assert!(!DeterministicPit::handles_port(0x80));
        assert!(!DeterministicPit::handles_port(0x3F));
    }

    #[test]
    fn test_channel0_mode2_irq() {
        let mut pit = DeterministicPit::new(TEST_TSC_KHZ);
        let start_tsc = 1000000u64;

        // Program channel 0 in mode 2 with reload value 1193 (1ms period at PIT freq)
        // Command: channel 0, lo/hi byte, mode 2
        pit.write_port(PIT_PORT_COMMAND, 0b00110100, start_tsc);
        pit.write_port(PIT_PORT_CHANNEL0, 0xA9, start_tsc); // 1193 & 0xFF
        pit.write_port(PIT_PORT_CHANNEL0, 0x04, start_tsc); // 1193 >> 8

        // Initially no IRQ pending
        assert!(!pit.pending_irq(start_tsc));

        // Calculate TSC ticks for one PIT period (1193 PIT ticks).
        // Use ceiling division to ensure the period is fully elapsed:
        // tsc_ticks = ceil(pit_ticks * TSC_FREQ / PIT_FREQ)
        let tsc_per_period =
            (1193u128 * TEST_TSC_KHZ as u128 * 1000).div_ceil(PIT_FREQ_HZ as u128) as u64;

        // Just before first period: no IRQ
        assert!(!pit.pending_irq(start_tsc + tsc_per_period - 1));

        // At first period: IRQ should be pending
        assert!(pit.pending_irq(start_tsc + tsc_per_period));

        // Acknowledge IRQ
        pit.acknowledge_irq();
        assert_eq!(pit.irqs_delivered, 1);

        // IRQ no longer pending (same period)
        assert!(!pit.pending_irq(start_tsc + tsc_per_period));

        // At second period: another IRQ
        assert!(pit.pending_irq(start_tsc + tsc_per_period * 2));
    }

    #[test]
    fn test_channel2_mode0_calibration() {
        let mut pit = DeterministicPit::new(TEST_TSC_KHZ);
        let start_tsc = 5000000u64;

        // Enable channel 2 gate via port 0x61
        pit.write_port(PORT_SYSTEM_CONTROL_B, 0x01, start_tsc);

        // Program channel 2 in mode 0 with reload value 0xFFFF (max count)
        // Command: channel 2, lo/hi byte, mode 0
        pit.write_port(PIT_PORT_COMMAND, 0b10110000, start_tsc);
        pit.write_port(PIT_PORT_CHANNEL2, 0xFF, start_tsc);
        pit.write_port(PIT_PORT_CHANNEL2, 0xFF, start_tsc);

        // Initially output should be LOW
        let port_61 = pit.read_port(PORT_SYSTEM_CONTROL_B, start_tsc);
        assert_eq!(port_61 & 0x20, 0); // Bit 5 should be 0

        // Calculate TSC for 65535 PIT ticks (ceiling division)
        let tsc_for_max_count =
            (65535u128 * TEST_TSC_KHZ as u128 * 1000).div_ceil(PIT_FREQ_HZ as u128) as u64;

        // Just before completion: still LOW
        let port_61 = pit.read_port(PORT_SYSTEM_CONTROL_B, start_tsc + tsc_for_max_count - 1);
        assert_eq!(port_61 & 0x20, 0);

        // At completion: output goes HIGH
        let port_61 = pit.read_port(PORT_SYSTEM_CONTROL_B, start_tsc + tsc_for_max_count);
        assert_eq!(port_61 & 0x20, 0x20); // Bit 5 should be 1
    }

    #[test]
    fn test_port_61_readback() {
        let mut pit = DeterministicPit::new(TEST_TSC_KHZ);
        let tsc = 1000u64;

        // Write gate and speaker bits
        pit.write_port(PORT_SYSTEM_CONTROL_B, 0x03, tsc);
        let val = pit.read_port(PORT_SYSTEM_CONTROL_B, tsc);
        assert_eq!(val & 0x03, 0x03); // Bits 0-1 should be set

        // Bit 4 (refresh) should toggle on each read
        let val1 = pit.read_port(PORT_SYSTEM_CONTROL_B, tsc);
        let val2 = pit.read_port(PORT_SYSTEM_CONTROL_B, tsc);
        assert_ne!(val1 & 0x10, val2 & 0x10);
    }

    #[test]
    fn test_latch_command() {
        let mut pit = DeterministicPit::new(TEST_TSC_KHZ);
        let start_tsc = 2000000u64;

        // Program channel 0 in mode 2 with reload 1000
        pit.write_port(PIT_PORT_COMMAND, 0b00110100, start_tsc);
        pit.write_port(PIT_PORT_CHANNEL0, 0xE8, start_tsc); // 1000 & 0xFF
        pit.write_port(PIT_PORT_CHANNEL0, 0x03, start_tsc); // 1000 >> 8

        // Advance TSC to simulate some elapsed time
        let elapsed_tsc = 100000u64;
        let tsc = start_tsc + elapsed_tsc;

        // Issue latch command
        pit.write_port(PIT_PORT_COMMAND, 0b00000000, tsc);

        // Read latched value (should be stable even if TSC advances)
        let lo = pit.read_port(PIT_PORT_CHANNEL0, tsc + 50000);
        let hi = pit.read_port(PIT_PORT_CHANNEL0, tsc + 100000);
        let latched = (hi as u16) << 8 | (lo as u16);

        // The latched value should be close to the computed value at latch time
        let pit_ticks_at_latch = pit.tsc_to_pit_ticks(elapsed_tsc);
        let expected = if pit_ticks_at_latch >= 1000 {
            1000 - (pit_ticks_at_latch % 1000)
        } else {
            1000 - pit_ticks_at_latch
        };
        assert_eq!(latched as u64, expected);
    }

    #[test]
    fn test_lobyte_hibyte_write() {
        let mut pit = DeterministicPit::new(TEST_TSC_KHZ);
        let tsc = 5000u64;

        // Program channel 1 in mode 3, lo/hi byte access
        pit.write_port(PIT_PORT_COMMAND, 0b01110110, tsc);

        // Channel should not be armed after first byte
        assert!(!pit.channels[1].armed);

        // Write low byte
        pit.write_port(PIT_PORT_CHANNEL1, 0x34, tsc);
        assert!(!pit.channels[1].armed);

        // Write high byte
        pit.write_port(PIT_PORT_CHANNEL1, 0x12, tsc);
        assert!(pit.channels[1].armed);
        assert_eq!(pit.channels[1].reload, 0x1234);
    }

    #[test]
    fn test_multiple_irq_periods() {
        let mut pit = DeterministicPit::new(TEST_TSC_KHZ);
        let start_tsc = 0u64;

        // Program channel 0 in mode 2 with small reload value
        pit.write_port(PIT_PORT_COMMAND, 0b00110100, start_tsc);
        pit.write_port(PIT_PORT_CHANNEL0, 100, start_tsc); // reload = 100
        pit.write_port(PIT_PORT_CHANNEL0, 0, start_tsc);

        let tsc_per_period =
            (100u128 * TEST_TSC_KHZ as u128 * 1000).div_ceil(PIT_FREQ_HZ as u128) as u64;

        // Check multiple periods
        for period in 1..=5 {
            let tsc = start_tsc + tsc_per_period * period;
            assert!(pit.pending_irq(tsc), "Period {} should have pending IRQ", period);
            pit.acknowledge_irq();
            assert_eq!(pit.irqs_delivered, period);
            assert!(!pit.pending_irq(tsc), "After ack, period {} should not have pending IRQ", period);
        }
    }

    #[test]
    fn test_snapshot_restore() {
        let mut pit = DeterministicPit::new(TEST_TSC_KHZ);
        let tsc = 123456u64;

        // Program some state
        pit.write_port(PIT_PORT_COMMAND, 0b00110100, tsc);
        pit.write_port(PIT_PORT_CHANNEL0, 0xCD, tsc);
        pit.write_port(PIT_PORT_CHANNEL0, 0xAB, tsc);
        pit.write_port(PORT_SYSTEM_CONTROL_B, 0x03, tsc);
        pit.irqs_delivered = 42;

        // Take snapshot
        let snapshot = pit.snapshot();

        // Verify snapshot contents
        assert_eq!(snapshot.tsc_khz, TEST_TSC_KHZ);
        assert_eq!(snapshot.irqs_delivered, 42);
        assert_eq!(snapshot.channels[0].reload, 0xABCD);
        assert_eq!(snapshot.channels[0].mode, ChannelMode::Mode2);
        assert!(snapshot.channels[0].armed);

        // Restore and verify
        let restored = DeterministicPit::restore(&snapshot);
        let snapshot2 = restored.snapshot();
        assert_eq!(snapshot, snapshot2);
    }

    #[test]
    fn test_mode0_single_interrupt() {
        let mut pit = DeterministicPit::new(TEST_TSC_KHZ);
        let start_tsc = 0u64;

        // Program channel 0 in mode 0
        pit.write_port(PIT_PORT_COMMAND, 0b00110000, start_tsc);
        pit.write_port(PIT_PORT_CHANNEL0, 100, start_tsc);
        pit.write_port(PIT_PORT_CHANNEL0, 0, start_tsc);

        let tsc_for_count =
            (100u128 * TEST_TSC_KHZ as u128 * 1000).div_ceil(PIT_FREQ_HZ as u128) as u64;

        // No IRQ initially
        assert!(!pit.pending_irq(start_tsc));

        // IRQ at terminal count
        assert!(pit.pending_irq(start_tsc + tsc_for_count));
        pit.acknowledge_irq();

        // No further IRQs in mode 0 (one-shot)
        assert!(!pit.pending_irq(start_tsc + tsc_for_count * 2));
    }

    #[test]
    fn test_gate_control_channel2() {
        let mut pit = DeterministicPit::new(TEST_TSC_KHZ);
        let tsc = 1000u64;

        // Channel 2 gate starts low
        assert!(!pit.channels[2].gate);

        // Program channel 2
        pit.write_port(PIT_PORT_COMMAND, 0b10110100, tsc);
        pit.write_port(PIT_PORT_CHANNEL2, 100, tsc);
        pit.write_port(PIT_PORT_CHANNEL2, 0, tsc);

        // Output should be false when gate is low
        let val = pit.read_port(PORT_SYSTEM_CONTROL_B, tsc);
        assert_eq!(val & 0x20, 0);

        // Enable gate
        pit.write_port(PORT_SYSTEM_CONTROL_B, 0x01, tsc);
        assert!(pit.channels[2].gate);

        // Now output can be active
        let tsc_later = tsc + 1000000;
        let val = pit.read_port(PORT_SYSTEM_CONTROL_B, tsc_later);
        // In mode 2, output should be high (we're past the initial pulse)
        assert_eq!(val & 0x20, 0x20);
    }
}
