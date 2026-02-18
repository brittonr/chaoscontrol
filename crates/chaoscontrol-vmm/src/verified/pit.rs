//! Pure, verifiable functions for PIT (Programmable Interval Timer) emulation.
//!
//! Every function in this module is:
//! - **Pure**: no I/O, no system calls, no side effects beyond the return value.
//! - **Deterministic**: same inputs always produce the same outputs.
//! - **Assertion-guarded**: Tiger Style `debug_assert!` preconditions and
//!   postconditions on every non-trivial function.
//!
//! These properties make the functions suitable for formal verification with
//! [Verus](https://github.com/verus-lang/verus).  The corresponding spec
//! file is `verus/pit_spec.rs`.
//!
//! # Mapping to `devices/pit.rs`
//!
//! | Verified function           | Original location in `devices/pit.rs`     |
//! |-----------------------------|-------------------------------------------|
//! | [`compute_counter`]         | `PitChannel::compute_counter()`           |
//! | [`compute_output`]          | `PitChannel::output()`                    |
//! | [`tsc_to_pit_ticks`]        | `DeterministicPit::tsc_to_pit_ticks()`    |
//! | [`pending_irq_check`]       | `DeterministicPit::pending_irq()` (logic) |
//! | [`next_irq_tsc_compute`]    | `DeterministicPit::next_irq_tsc()`        |
//! | [`decode_command`]          | `DeterministicPit::write_command()` parse |
//! | [`handles_port`]            | `DeterministicPit::handles_port()`        |

use crate::devices::pit::ChannelMode;

// ─── Constants (duplicated from devices/pit.rs so this module is self-contained) ─

/// PIT operating frequency in Hz (1.193182 MHz).
///
/// Must be identical to the value in `devices/pit.rs`.  A compile-time test at the
/// bottom of this file enforces this.
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

// Compile-time assertion that PIT frequency is positive
const _: () = assert!(PIT_FREQ_HZ > 0, "PIT frequency must be positive");

// ─── Counter computation ────────────────────────────────────────────

/// Compute the current PIT counter value based on elapsed ticks.
///
/// The counter counts down from the reload value. In periodic modes
/// (Mode2, Mode3), the counter wraps around. In one-shot modes (Mode0),
/// it saturates at 0.
///
/// # Arguments
///
/// * `mode` - Channel operating mode
/// * `reload` - Reload/initial count value (0 means 65536)
/// * `armed` - Whether the channel is armed (counting)
/// * `elapsed_pit_ticks` - Number of PIT ticks elapsed since arming
///
/// # Returns
///
/// The current counter value (0-65535)
///
/// # Properties verified by `verus/pit_spec.rs`
///
/// - `!armed  ⟹  result == 0`
/// - `result <= effective_reload` (where reload 0 means 65536)
/// - Mode0 saturates at 0 (doesn't wrap)
/// - Mode2/Mode3 wrap periodically
///
/// # Examples
///
/// ```rust,ignore
/// use crate::verified::pit::compute_counter;
/// use crate::devices::pit::ChannelMode;
///
/// // Mode0: count down once, saturate at 0
/// assert_eq!(compute_counter(ChannelMode::Mode0, 1000, true, 0), 1000);
/// assert_eq!(compute_counter(ChannelMode::Mode0, 1000, true, 500), 500);
/// assert_eq!(compute_counter(ChannelMode::Mode0, 1000, true, 1000), 0);
/// assert_eq!(compute_counter(ChannelMode::Mode0, 1000, true, 2000), 0);
///
/// // Mode2: periodic, wraps around
/// assert_eq!(compute_counter(ChannelMode::Mode2, 1000, true, 1000), 1000);
/// assert_eq!(compute_counter(ChannelMode::Mode2, 1000, true, 1001), 999);
/// ```
pub fn compute_counter(
    mode: ChannelMode,
    reload: u16,
    armed: bool,
    elapsed_pit_ticks: u64,
) -> u16 {
    // Precondition: if not armed, always return 0
    if !armed {
        return 0;
    }

    let effective_reload = if reload == 0 { 65536u64 } else { reload as u64 };

    let result = match mode {
        ChannelMode::Mode0 => {
            // Count down once, saturate at 0
            if elapsed_pit_ticks >= effective_reload {
                0
            } else {
                (effective_reload - elapsed_pit_ticks) as u16
            }
        }
        ChannelMode::Mode2 | ChannelMode::Mode3 => {
            // Periodic mode: count wraps around
            let position = elapsed_pit_ticks % effective_reload;
            if position == 0 && elapsed_pit_ticks > 0 {
                effective_reload as u16
            } else {
                (effective_reload - position) as u16
            }
        }
        _ => {
            // Other modes not fully implemented, return reload value
            reload
        }
    };

    // Postcondition: result is bounded by effective reload
    debug_assert!(
        result as u64 <= effective_reload,
        "counter {} must not exceed effective reload {}",
        result,
        effective_reload
    );

    result
}

// ─── Output signal computation ──────────────────────────────────────

/// Compute the output signal state for a PIT channel.
///
/// The output signal behavior depends on the channel mode:
/// - Mode 0: LOW until terminal count, then HIGH
/// - Mode 2: HIGH except for one tick pulse at reload
/// - Mode 3: Square wave (HIGH for first half, LOW for second half)
///
/// # Arguments
///
/// * `mode` - Channel operating mode
/// * `armed` - Whether the channel is armed
/// * `gate` - Gate input signal (must be HIGH for output to be active)
/// * `reload` - Reload/initial count value (0 means 65536)
/// * `elapsed_pit_ticks` - Number of PIT ticks elapsed since arming
///
/// # Returns
///
/// `true` if output is HIGH, `false` if LOW
///
/// # Properties verified by `verus/pit_spec.rs`
///
/// - `!armed  ⟹  result == false`
/// - `!gate   ⟹  result == false`
/// - Mode0: LOW before terminal count, HIGH after
/// - Mode2: HIGH except for one tick at position 0
/// - Mode3: square wave with 50% duty cycle
///
/// # Examples
///
/// ```rust,ignore
/// use crate::verified::pit::compute_output;
/// use crate::devices::pit::ChannelMode;
///
/// // Unarmed or gate-off: always LOW
/// assert_eq!(compute_output(ChannelMode::Mode0, false, true, 1000, 0), false);
/// assert_eq!(compute_output(ChannelMode::Mode0, true, false, 1000, 0), false);
///
/// // Mode0: LOW before terminal count, HIGH after
/// assert_eq!(compute_output(ChannelMode::Mode0, true, true, 1000, 0), false);
/// assert_eq!(compute_output(ChannelMode::Mode0, true, true, 1000, 1000), true);
/// ```
pub fn compute_output(
    mode: ChannelMode,
    armed: bool,
    gate: bool,
    reload: u16,
    elapsed_pit_ticks: u64,
) -> bool {
    // Precondition: if not armed or gate is off, output is always LOW
    if !armed || !gate {
        return false;
    }

    let effective_reload = if reload == 0 { 65536u64 } else { reload as u64 };

    match mode {
        ChannelMode::Mode0 => {
            // Output starts LOW, goes HIGH when counter reaches 0
            elapsed_pit_ticks >= effective_reload
        }
        ChannelMode::Mode2 => {
            // Output is HIGH except for 1 tick pulse LOW at reload
            let position = elapsed_pit_ticks % effective_reload;
            position != 0 || elapsed_pit_ticks == 0
        }
        ChannelMode::Mode3 => {
            // Square wave: HIGH for first half, LOW for second half
            let position = elapsed_pit_ticks % effective_reload;
            position < (effective_reload / 2)
        }
        _ => true,
    }
}

// ─── TSC to PIT conversion ──────────────────────────────────────────

/// Convert elapsed TSC ticks to PIT ticks.
///
/// PIT runs at 1,193,182 Hz while TSC runs at a configurable frequency.
/// This function performs the conversion: `pit_ticks = tsc * PIT_FREQ / TSC_FREQ`
///
/// Uses u128 arithmetic internally to avoid overflow.
///
/// # Arguments
///
/// * `elapsed_tsc` - Number of TSC ticks elapsed
/// * `tsc_khz` - TSC frequency in KHz
///
/// # Returns
///
/// Number of PIT ticks elapsed
///
/// # Panics (debug only)
///
/// - `tsc_khz` must be non-zero (division by zero)
///
/// # Properties verified by `verus/pit_spec.rs`
///
/// - Monotonic: `tsc1 <= tsc2  ⟹  result1 <= result2`
/// - `elapsed_tsc == 0  ⟹  result == 0`
/// - Result is bounded by reasonable limits
///
/// # Examples
///
/// ```rust,ignore
/// use crate::verified::pit::tsc_to_pit_ticks;
///
/// // At 2.4 GHz TSC, ~2011 TSC ticks per PIT tick
/// assert_eq!(tsc_to_pit_ticks(0, 2_400_000), 0);
/// assert!(tsc_to_pit_ticks(2011, 2_400_000) <= 1);
/// ```
pub fn tsc_to_pit_ticks(elapsed_tsc: u64, tsc_khz: u32) -> u64 {
    // Precondition: TSC frequency must be non-zero
    debug_assert!(tsc_khz > 0, "tsc_khz must be non-zero");

    // Use u128 to avoid overflow: pit_ticks = tsc * PIT_FREQ / TSC_FREQ
    let tsc_freq = tsc_khz as u128 * 1000;
    let pit_ticks = (elapsed_tsc as u128 * PIT_FREQ_HZ as u128) / tsc_freq;
    let result = pit_ticks as u64;

    // Postcondition: zero input gives zero output
    debug_assert!(
        elapsed_tsc != 0 || result == 0,
        "zero TSC must produce zero PIT ticks"
    );

    result
}

// ─── IRQ pending check ──────────────────────────────────────────────

/// Check if an IRQ should be delivered based on elapsed time.
///
/// This is the core IRQ decision logic:
/// - Mode2/Mode3: IRQ fires periodically every `reload` PIT ticks
/// - Mode0/Mode1/Mode4/Mode5: IRQ fires once at terminal count
///
/// # Arguments
///
/// * `mode` - Channel operating mode
/// * `armed` - Whether the channel is armed
/// * `reload` - Reload/initial count value (0 means 65536)
/// * `elapsed_pit_ticks` - Number of PIT ticks elapsed since arming
/// * `irqs_delivered` - Number of IRQs already delivered
///
/// # Returns
///
/// `true` if an IRQ should be delivered now
///
/// # Properties verified by `verus/pit_spec.rs`
///
/// - `!armed  ⟹  result == false`
/// - Mode0: fires at most once (when elapsed >= reload and irqs_delivered == 0)
/// - Mode2: fires periodically (when periods_elapsed > irqs_delivered)
///
/// # Examples
///
/// ```rust,ignore
/// use crate::verified::pit::pending_irq_check;
/// use crate::devices::pit::ChannelMode;
///
/// // Mode0: fires once at terminal count
/// assert_eq!(pending_irq_check(ChannelMode::Mode0, true, 1000, 999, 0), false);
/// assert_eq!(pending_irq_check(ChannelMode::Mode0, true, 1000, 1000, 0), true);
/// assert_eq!(pending_irq_check(ChannelMode::Mode0, true, 1000, 1000, 1), false);
///
/// // Mode2: fires periodically
/// assert_eq!(pending_irq_check(ChannelMode::Mode2, true, 1000, 1000, 0), true);
/// assert_eq!(pending_irq_check(ChannelMode::Mode2, true, 1000, 2000, 1), true);
/// ```
pub fn pending_irq_check(
    mode: ChannelMode,
    armed: bool,
    reload: u16,
    elapsed_pit_ticks: u64,
    irqs_delivered: u64,
) -> bool {
    // Precondition: if not armed, no IRQ can be pending
    if !armed {
        return false;
    }

    let effective_reload = if reload == 0 { 65536u64 } else { reload as u64 };

    match mode {
        ChannelMode::Mode2 | ChannelMode::Mode3 => {
            // Periodic: IRQ fires every reload period
            let periods_elapsed = elapsed_pit_ticks / effective_reload;
            periods_elapsed > irqs_delivered
        }
        ChannelMode::Mode0 | ChannelMode::Mode1 | ChannelMode::Mode4 | ChannelMode::Mode5 => {
            // One-shot: IRQ fires once when counter reaches 0
            elapsed_pit_ticks >= effective_reload && irqs_delivered == 0
        }
    }
}

// ─── Next IRQ TSC computation ───────────────────────────────────────

/// Compute the TSC value at which the next IRQ will fire.
///
/// Returns `None` if the channel won't generate another IRQ (not armed,
/// or one-shot mode already fired).
///
/// # Arguments
///
/// * `mode` - Channel operating mode
/// * `armed` - Whether the channel is armed
/// * `reload` - Reload/initial count value (0 means 65536)
/// * `start_tsc` - TSC value when channel was armed
/// * `tsc_khz` - TSC frequency in KHz
/// * `irqs_delivered` - Number of IRQs already delivered
///
/// # Returns
///
/// `Some(tsc)` with the TSC value of the next IRQ, or `None` if no more IRQs
///
/// # Panics (debug only)
///
/// - `tsc_khz` must be non-zero
///
/// # Properties verified by `verus/pit_spec.rs`
///
/// - `!armed  ⟹  result == None`
/// - Mode0 after first IRQ: returns None
/// - Mode2: always returns Some (periodic)
/// - `result == Some(tsc)  ⟹  tsc >= start_tsc`
///
/// # Examples
///
/// ```rust,ignore
/// use crate::verified::pit::next_irq_tsc_compute;
/// use crate::devices::pit::ChannelMode;
///
/// // Mode0: fires once, then None
/// let tsc = next_irq_tsc_compute(ChannelMode::Mode0, true, 1000, 0, 2_400_000, 0);
/// assert!(tsc.is_some());
/// let tsc2 = next_irq_tsc_compute(ChannelMode::Mode0, true, 1000, 0, 2_400_000, 1);
/// assert_eq!(tsc2, None);
///
/// // Mode2: always has next IRQ
/// let tsc = next_irq_tsc_compute(ChannelMode::Mode2, true, 1000, 0, 2_400_000, 5);
/// assert!(tsc.is_some());
/// ```
pub fn next_irq_tsc_compute(
    mode: ChannelMode,
    armed: bool,
    reload: u16,
    start_tsc: u64,
    tsc_khz: u32,
    irqs_delivered: u64,
) -> Option<u64> {
    // Precondition: if not armed, no next IRQ
    if !armed {
        return None;
    }

    debug_assert!(tsc_khz > 0, "tsc_khz must be non-zero");

    let effective_reload = if reload == 0 { 65536u64 } else { reload as u64 };
    let tsc_hz = tsc_khz as u128 * 1000;

    let result = match mode {
        ChannelMode::Mode2 | ChannelMode::Mode3 => {
            // Periodic modes: next IRQ fires after (irqs_delivered + 1) full periods
            let next_period = irqs_delivered + 1;
            let pit_ticks_needed = next_period * effective_reload;
            let tsc_offset = (pit_ticks_needed as u128 * tsc_hz).div_ceil(PIT_FREQ_HZ as u128) as u64;
            Some(start_tsc + tsc_offset)
        }
        ChannelMode::Mode0 | ChannelMode::Mode1 | ChannelMode::Mode4 | ChannelMode::Mode5 => {
            // One-shot modes: fire once at terminal count
            if irqs_delivered > 0 {
                None // Already fired
            } else {
                let tsc_offset = (effective_reload as u128 * tsc_hz).div_ceil(PIT_FREQ_HZ as u128) as u64;
                Some(start_tsc + tsc_offset)
            }
        }
    };

    // Postcondition: if Some, result >= start_tsc
    debug_assert!(
        result.is_none() || result.unwrap() >= start_tsc,
        "next IRQ TSC {} must be >= start TSC {}",
        result.unwrap(),
        start_tsc
    );

    result
}

// ─── Command byte decoding ──────────────────────────────────────────

/// Decode a PIT command byte into its components.
///
/// The command byte format (from Intel 8254 datasheet):
/// - Bits 7-6: Channel select (00=ch0, 01=ch1, 10=ch2, 11=readback)
/// - Bits 5-4: Access mode (00=latch, 01=lobyte, 10=hibyte, 11=lohi)
/// - Bits 3-1: Operating mode (000=mode0, 001=mode1, x10=mode2, x11=mode3, 100=mode4, 101=mode5)
/// - Bit 0: BCD mode (ignored, always binary)
///
/// # Arguments
///
/// * `value` - Command byte
///
/// # Returns
///
/// Tuple of `(channel_select, access_mode, mode)` where:
/// - `channel_select` is 0-2 for channel number, 3 for readback
/// - `access_mode` is 0-3 (0=latch, 1=lobyte, 2=hibyte, 3=lohi)
/// - `mode` is the decoded ChannelMode
///
/// # Examples
///
/// ```rust,ignore
/// use crate::verified::pit::decode_command;
/// use crate::devices::pit::ChannelMode;
///
/// // Channel 0, lo/hi byte, mode 2
/// let (ch, access, mode) = decode_command(0b00110100);
/// assert_eq!(ch, 0);
/// assert_eq!(access, 3);
/// assert_eq!(mode, ChannelMode::Mode2);
/// ```
pub fn decode_command(value: u8) -> (u8, u8, ChannelMode) {
    let channel_select = (value >> 6) & 0x3;
    let access_mode = (value >> 4) & 0x3;
    let mode_bits = (value >> 1) & 0x7;

    let mode = match mode_bits {
        0 => ChannelMode::Mode0,
        1 => ChannelMode::Mode1,
        2 | 6 => ChannelMode::Mode2,
        3 | 7 => ChannelMode::Mode3,
        4 => ChannelMode::Mode4,
        5 => ChannelMode::Mode5,
        _ => ChannelMode::Mode0, // Shouldn't happen with 3 bits
    };

    // Postcondition: channel_select is 0-3
    debug_assert!(channel_select <= 3, "channel_select must be 0-3");
    // Postcondition: access_mode is 0-3
    debug_assert!(access_mode <= 3, "access_mode must be 0-3");

    (channel_select, access_mode, mode)
}

// ─── Port handling check ────────────────────────────────────────────

/// Check if the PIT handles a given I/O port.
///
/// The PIT responds to ports 0x40-0x43 (channel data and command) and
/// port 0x61 (system control port B).
///
/// # Arguments
///
/// * `port` - I/O port address
///
/// # Returns
///
/// `true` if this is a PIT-related port
///
/// # Examples
///
/// ```rust,ignore
/// use crate::verified::pit::{handles_port, PIT_PORT_CHANNEL0, PIT_PORT_COMMAND};
///
/// assert_eq!(handles_port(PIT_PORT_CHANNEL0), true);
/// assert_eq!(handles_port(PIT_PORT_COMMAND), true);
/// assert_eq!(handles_port(0x80), false);
/// ```
pub fn handles_port(port: u16) -> bool {
    matches!(
        port,
        PIT_PORT_CHANNEL0
            | PIT_PORT_CHANNEL1
            | PIT_PORT_CHANNEL2
            | PIT_PORT_COMMAND
            | PORT_SYSTEM_CONTROL_B
    )
}

// ─── Compile-time tests ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ─── compute_counter tests ──────────────────────────────────────

    #[test]
    fn test_compute_counter_unarmed_returns_zero() {
        // Tiger Style: unarmed channel must return 0
        for mode in [
            ChannelMode::Mode0,
            ChannelMode::Mode1,
            ChannelMode::Mode2,
            ChannelMode::Mode3,
            ChannelMode::Mode4,
            ChannelMode::Mode5,
        ] {
            assert_eq!(compute_counter(mode, 1000, false, 0), 0);
            assert_eq!(compute_counter(mode, 1000, false, 500), 0);
            assert_eq!(compute_counter(mode, 1000, false, 10000), 0);
        }
    }

    #[test]
    fn test_compute_counter_mode0_saturates() {
        let reload = 1000u16;

        // Count down
        assert_eq!(compute_counter(ChannelMode::Mode0, reload, true, 0), 1000);
        assert_eq!(compute_counter(ChannelMode::Mode0, reload, true, 500), 500);
        assert_eq!(compute_counter(ChannelMode::Mode0, reload, true, 999), 1);

        // Saturate at 0
        assert_eq!(compute_counter(ChannelMode::Mode0, reload, true, 1000), 0);
        assert_eq!(compute_counter(ChannelMode::Mode0, reload, true, 1500), 0);
        assert_eq!(compute_counter(ChannelMode::Mode0, reload, true, 10000), 0);
    }

    #[test]
    fn test_compute_counter_mode2_wraps() {
        let reload = 1000u16;

        // First period
        assert_eq!(compute_counter(ChannelMode::Mode2, reload, true, 0), 1000);
        assert_eq!(compute_counter(ChannelMode::Mode2, reload, true, 1), 999);
        assert_eq!(compute_counter(ChannelMode::Mode2, reload, true, 999), 1);

        // Wrap at reload
        assert_eq!(compute_counter(ChannelMode::Mode2, reload, true, 1000), 1000);
        assert_eq!(compute_counter(ChannelMode::Mode2, reload, true, 1001), 999);
    }

    #[test]
    fn test_compute_counter_bounded() {
        // Tiger Style: counter must never exceed effective_reload
        for reload in [1, 100, 1000, 65535] {
            for elapsed in [0, 1, 100, 1000, 10000, 65536, 100000] {
                let effective_reload = if reload == 0 { 65536u64 } else { reload as u64 };
                let count = compute_counter(ChannelMode::Mode2, reload, true, elapsed);
                assert!(
                    count as u64 <= effective_reload,
                    "counter {} exceeds reload {}",
                    count,
                    effective_reload
                );
            }
        }
    }

    #[test]
    fn test_compute_counter_reload_zero_means_65536() {
        // Reload value 0 means 65536
        // At elapsed=0, Mode0 returns (65536 - 0) as u16 = 0 (wraps)
        let count = compute_counter(ChannelMode::Mode0, 0, true, 0);
        assert_eq!(count, 0); // 65536 wraps to 0 in u16

        // At elapsed=1, returns (65536 - 1) = 65535
        let count = compute_counter(ChannelMode::Mode0, 0, true, 1);
        assert_eq!(count, 65535);

        // Mode2 at elapsed=0 returns effective_reload as u16 = 0
        let count = compute_counter(ChannelMode::Mode2, 0, true, 0);
        assert_eq!(count, 0);

        // Mode2 at elapsed=1 returns (65536 - 1) = 65535
        let count = compute_counter(ChannelMode::Mode2, 0, true, 1);
        assert_eq!(count, 65535);
    }

    // ─── compute_output tests ───────────────────────────────────────

    #[test]
    fn test_compute_output_unarmed_returns_false() {
        // Tiger Style: unarmed channel must return false
        for mode in [ChannelMode::Mode0, ChannelMode::Mode2, ChannelMode::Mode3] {
            assert!(!compute_output(mode, false, true, 1000, 0));
            assert!(!compute_output(mode, false, true, 1000, 1000));
        }
    }

    #[test]
    fn test_compute_output_gate_off_returns_false() {
        // Tiger Style: gate-off channel must return false
        for mode in [ChannelMode::Mode0, ChannelMode::Mode2, ChannelMode::Mode3] {
            assert!(!compute_output(mode, true, false, 1000, 0));
            assert!(!compute_output(mode, true, false, 1000, 1000));
        }
    }

    #[test]
    fn test_compute_output_mode0() {
        let reload = 1000u16;

        // Output LOW before terminal count
        assert!(!compute_output(ChannelMode::Mode0, true, true, reload, 0));
        assert!(!compute_output(ChannelMode::Mode0, true, true, reload, 500));
        assert!(!compute_output(ChannelMode::Mode0, true, true, reload, 999));

        // Output HIGH at/after terminal count
        assert!(compute_output(ChannelMode::Mode0, true, true, reload, 1000));
        assert!(compute_output(ChannelMode::Mode0, true, true, reload, 1500));
    }

    #[test]
    fn test_compute_output_mode2() {
        let reload = 1000u16;

        // Output HIGH at start
        assert!(compute_output(ChannelMode::Mode2, true, true, reload, 0));

        // Output HIGH during countdown
        assert!(compute_output(ChannelMode::Mode2, true, true, reload, 1));
        assert!(compute_output(ChannelMode::Mode2, true, true, reload, 500));
        assert!(compute_output(ChannelMode::Mode2, true, true, reload, 999));

        // Output LOW at reload point
        assert!(!compute_output(ChannelMode::Mode2, true, true, reload, 1000));

        // Output HIGH after reload
        assert!(compute_output(ChannelMode::Mode2, true, true, reload, 1001));
    }

    #[test]
    fn test_compute_output_mode3_square_wave() {
        let reload = 1000u16;

        // HIGH for first half
        assert!(compute_output(ChannelMode::Mode3, true, true, reload, 0));
        assert!(compute_output(ChannelMode::Mode3, true, true, reload, 499));

        // LOW for second half
        assert!(!compute_output(ChannelMode::Mode3, true, true, reload, 500));
        assert!(!compute_output(ChannelMode::Mode3, true, true, reload, 999));

        // Wrap: HIGH again
        assert!(compute_output(ChannelMode::Mode3, true, true, reload, 1000));
    }

    // ─── tsc_to_pit_ticks tests ─────────────────────────────────────

    #[test]
    fn test_tsc_to_pit_ticks_zero() {
        assert_eq!(tsc_to_pit_ticks(0, 2_400_000), 0);
        assert_eq!(tsc_to_pit_ticks(0, 3_000_000), 0);
    }

    #[test]
    fn test_tsc_to_pit_ticks_monotonic() {
        // Tiger Style: must be monotonic
        let tsc_khz = 2_400_000;
        let tsc_values = [0, 1, 100, 1000, 10000, 100000, 1000000];

        for i in 1..tsc_values.len() {
            let pit1 = tsc_to_pit_ticks(tsc_values[i - 1], tsc_khz);
            let pit2 = tsc_to_pit_ticks(tsc_values[i], tsc_khz);
            assert!(
                pit2 >= pit1,
                "TSC->PIT not monotonic: {} -> {}, {} -> {}",
                tsc_values[i - 1],
                pit1,
                tsc_values[i],
                pit2
            );
        }
    }

    #[test]
    fn test_tsc_to_pit_ticks_known_values() {
        // At 2.4 GHz TSC and 1.193182 MHz PIT:
        // 1 PIT tick should take ~2011 TSC ticks
        let tsc_khz = 2_400_000;

        // Small values
        let pit = tsc_to_pit_ticks(2011, tsc_khz);
        assert!(pit <= 1, "2011 TSC ticks should be ~1 PIT tick, got {}", pit);

        // Larger values
        let pit = tsc_to_pit_ticks(2_011_420, tsc_khz);
        assert!(
            (999..=1001).contains(&pit),
            "~2M TSC ticks should be ~1000 PIT ticks, got {}",
            pit
        );
    }

    // ─── pending_irq_check tests ────────────────────────────────────

    #[test]
    fn test_pending_irq_unarmed_returns_false() {
        // Tiger Style: unarmed channel never has pending IRQ
        assert!(!pending_irq_check(ChannelMode::Mode0, false, 1000, 1000, 0));
        assert!(!pending_irq_check(ChannelMode::Mode2, false, 1000, 1000, 0));
    }

    #[test]
    fn test_pending_irq_mode0_fires_once() {
        let reload = 1000u16;

        // No IRQ before terminal count
        assert!(!pending_irq_check(ChannelMode::Mode0, true, reload, 0, 0));
        assert!(!pending_irq_check(ChannelMode::Mode0, true, reload, 500, 0));
        assert!(!pending_irq_check(ChannelMode::Mode0, true, reload, 999, 0));

        // IRQ at terminal count
        assert!(pending_irq_check(ChannelMode::Mode0, true, reload, 1000, 0));
        assert!(pending_irq_check(ChannelMode::Mode0, true, reload, 1500, 0));

        // No IRQ after acknowledged
        assert!(!pending_irq_check(ChannelMode::Mode0, true, reload, 1500, 1));
    }

    #[test]
    fn test_pending_irq_mode2_fires_periodically() {
        let reload = 1000u16;

        // No IRQ initially
        assert!(!pending_irq_check(ChannelMode::Mode2, true, reload, 0, 0));
        assert!(!pending_irq_check(ChannelMode::Mode2, true, reload, 999, 0));

        // First IRQ at 1*reload
        assert!(pending_irq_check(ChannelMode::Mode2, true, reload, 1000, 0));

        // No IRQ in same period after ack
        assert!(!pending_irq_check(ChannelMode::Mode2, true, reload, 1500, 1));

        // Second IRQ at 2*reload
        assert!(pending_irq_check(ChannelMode::Mode2, true, reload, 2000, 1));

        // Third IRQ at 3*reload
        assert!(pending_irq_check(ChannelMode::Mode2, true, reload, 3000, 2));
    }

    // ─── next_irq_tsc_compute tests ─────────────────────────────────

    #[test]
    fn test_next_irq_tsc_unarmed_returns_none() {
        // Tiger Style: unarmed channel has no next IRQ
        assert_eq!(
            next_irq_tsc_compute(ChannelMode::Mode0, false, 1000, 0, 2_400_000, 0),
            None
        );
        assert_eq!(
            next_irq_tsc_compute(ChannelMode::Mode2, false, 1000, 0, 2_400_000, 0),
            None
        );
    }

    #[test]
    fn test_next_irq_tsc_mode0_returns_terminal_count() {
        let reload = 1000u16;
        let start_tsc = 5000u64;
        let tsc_khz = 2_400_000;

        // First IRQ
        let next = next_irq_tsc_compute(ChannelMode::Mode0, true, reload, start_tsc, tsc_khz, 0);
        assert!(next.is_some());
        assert!(next.unwrap() > start_tsc);

        // After first IRQ: no more
        let next = next_irq_tsc_compute(ChannelMode::Mode0, true, reload, start_tsc, tsc_khz, 1);
        assert_eq!(next, None);
    }

    #[test]
    fn test_next_irq_tsc_mode2_returns_next_period() {
        let reload = 100u16;
        let start_tsc = 1000u64;
        let tsc_khz = 2_400_000;

        // Next IRQ for period 1
        let next1 = next_irq_tsc_compute(ChannelMode::Mode2, true, reload, start_tsc, tsc_khz, 0);
        assert!(next1.is_some());
        assert!(next1.unwrap() > start_tsc);

        // Next IRQ for period 2
        let next2 = next_irq_tsc_compute(ChannelMode::Mode2, true, reload, start_tsc, tsc_khz, 1);
        assert!(next2.is_some());
        assert!(next2.unwrap() > next1.unwrap());

        // Next IRQ for period 3
        let next3 = next_irq_tsc_compute(ChannelMode::Mode2, true, reload, start_tsc, tsc_khz, 2);
        assert!(next3.is_some());
        assert!(next3.unwrap() > next2.unwrap());
    }

    #[test]
    fn test_next_irq_tsc_always_gte_start() {
        // Tiger Style: result must be >= start_tsc when Some
        for mode in [ChannelMode::Mode0, ChannelMode::Mode2] {
            for start_tsc in [0, 1000, 1000000] {
                if let Some(next) = next_irq_tsc_compute(mode, true, 1000, start_tsc, 2_400_000, 0) {
                    assert!(
                        next >= start_tsc,
                        "next_irq_tsc {} < start_tsc {}",
                        next,
                        start_tsc
                    );
                }
            }
        }
    }

    // ─── decode_command tests ───────────────────────────────────────

    #[test]
    fn test_decode_command_channel_select() {
        // Channel 0
        let (ch, _, _) = decode_command(0b00000000);
        assert_eq!(ch, 0);

        // Channel 1
        let (ch, _, _) = decode_command(0b01000000);
        assert_eq!(ch, 1);

        // Channel 2
        let (ch, _, _) = decode_command(0b10000000);
        assert_eq!(ch, 2);

        // Readback
        let (ch, _, _) = decode_command(0b11000000);
        assert_eq!(ch, 3);
    }

    #[test]
    fn test_decode_command_access_mode() {
        // Latch
        let (_, access, _) = decode_command(0b00000000);
        assert_eq!(access, 0);

        // LoByte
        let (_, access, _) = decode_command(0b00010000);
        assert_eq!(access, 1);

        // HiByte
        let (_, access, _) = decode_command(0b00100000);
        assert_eq!(access, 2);

        // LoHiByte
        let (_, access, _) = decode_command(0b00110000);
        assert_eq!(access, 3);
    }

    #[test]
    fn test_decode_command_modes() {
        // Mode 0
        let (_, _, mode) = decode_command(0b00000000);
        assert_eq!(mode, ChannelMode::Mode0);

        // Mode 1
        let (_, _, mode) = decode_command(0b00000010);
        assert_eq!(mode, ChannelMode::Mode1);

        // Mode 2 (both aliases)
        let (_, _, mode) = decode_command(0b00000100);
        assert_eq!(mode, ChannelMode::Mode2);
        let (_, _, mode) = decode_command(0b00001100);
        assert_eq!(mode, ChannelMode::Mode2);

        // Mode 3 (both aliases)
        let (_, _, mode) = decode_command(0b00000110);
        assert_eq!(mode, ChannelMode::Mode3);
        let (_, _, mode) = decode_command(0b00001110);
        assert_eq!(mode, ChannelMode::Mode3);

        // Mode 4
        let (_, _, mode) = decode_command(0b00001000);
        assert_eq!(mode, ChannelMode::Mode4);

        // Mode 5
        let (_, _, mode) = decode_command(0b00001010);
        assert_eq!(mode, ChannelMode::Mode5);
    }

    #[test]
    fn test_decode_command_realistic_examples() {
        // Channel 0, LoHiByte, Mode 2 (common for IRQ 0)
        let (ch, access, mode) = decode_command(0b00110100);
        assert_eq!(ch, 0);
        assert_eq!(access, 3);
        assert_eq!(mode, ChannelMode::Mode2);

        // Channel 2, LoHiByte, Mode 0 (common for TSC calibration)
        let (ch, access, mode) = decode_command(0b10110000);
        assert_eq!(ch, 2);
        assert_eq!(access, 3);
        assert_eq!(mode, ChannelMode::Mode0);
    }

    // ─── handles_port tests ─────────────────────────────────────────

    #[test]
    fn test_handles_port() {
        // PIT ports
        assert!(handles_port(PIT_PORT_CHANNEL0));
        assert!(handles_port(PIT_PORT_CHANNEL1));
        assert!(handles_port(PIT_PORT_CHANNEL2));
        assert!(handles_port(PIT_PORT_COMMAND));
        assert!(handles_port(PORT_SYSTEM_CONTROL_B));

        // Non-PIT ports
        assert!(!handles_port(0x3F));
        assert!(!handles_port(0x44));
        assert!(!handles_port(0x60));
        assert!(!handles_port(0x62));
        assert!(!handles_port(0x80));
        assert!(!handles_port(0x3F8));
    }

    // ─── Constant consistency tests ─────────────────────────────────

    #[test]
    fn test_pit_freq_matches_devices_pit() {
        // Ensure constants match between verified and devices modules
        assert_eq!(PIT_FREQ_HZ, crate::devices::pit::PIT_FREQ_HZ);
        assert_eq!(PIT_PORT_CHANNEL0, crate::devices::pit::PIT_PORT_CHANNEL0);
        assert_eq!(PIT_PORT_CHANNEL1, crate::devices::pit::PIT_PORT_CHANNEL1);
        assert_eq!(PIT_PORT_CHANNEL2, crate::devices::pit::PIT_PORT_CHANNEL2);
        assert_eq!(PIT_PORT_COMMAND, crate::devices::pit::PIT_PORT_COMMAND);
        assert_eq!(PORT_SYSTEM_CONTROL_B, crate::devices::pit::PORT_SYSTEM_CONTROL_B);
    }
}
