//! Property-based tests for DeterministicPit.
//!
//! Key properties:
//! - Snapshot/restore round-trip preserves all state.
//! - Channel counter is always within valid range.
//! - Mode 2 IRQs are periodic: IRQ N fires at N * period.
//! - Port 0x61 refresh toggle alternates on every read.
//! - LoHiByte write sequence arms channel only after both bytes.

use chaoscontrol_vmm::devices::pit::{
    DeterministicPit, PIT_FREQ_HZ, PIT_PORT_CHANNEL0, PIT_PORT_COMMAND,
    PORT_SYSTEM_CONTROL_B,
};
use hegel::generators::*;
use hegel::TestCase;

const TEST_TSC_KHZ: u32 = 2_400_000;

/// TSC ticks for a given number of PIT ticks at our test frequency.
fn pit_to_tsc(pit_ticks: u64) -> u64 {
    (pit_ticks as u128 * TEST_TSC_KHZ as u128 * 1000).div_ceil(PIT_FREQ_HZ as u128) as u64
}

#[hegel::test(test_cases = 200)]
fn snapshot_restore_round_trip(tc: TestCase) {
    let mut pit = DeterministicPit::new(TEST_TSC_KHZ);
    let start_tsc = tc.draw(integers::<u64>().min_value(0).max_value(100_000_000));

    // Program channel 0 in mode 2
    pit.write_port(PIT_PORT_COMMAND, 0b00110100, start_tsc);
    let reload_lo = tc.draw(integers::<u8>());
    let reload_hi = tc.draw(integers::<u8>().min_value(1)); // nonzero high byte so reload > 255
    pit.write_port(PIT_PORT_CHANNEL0, reload_lo, start_tsc);
    pit.write_port(PIT_PORT_CHANNEL0, reload_hi, start_tsc);

    // Optionally program port 61
    let port_61_val = tc.draw(integers::<u8>().min_value(0).max_value(3));
    pit.write_port(PORT_SYSTEM_CONTROL_B, port_61_val, start_tsc);

    // Deliver some IRQs
    let irq_count = tc.draw(integers::<u64>().min_value(0).max_value(50));
    for _ in 0..irq_count {
        pit.acknowledge_irq();
    }

    let snap1 = pit.snapshot();
    let restored = DeterministicPit::restore(&snap1);
    let snap2 = restored.snapshot();

    assert_eq!(snap1, snap2, "snapshot round-trip must be lossless");
}

#[hegel::test(test_cases = 200)]
fn mode2_irqs_are_periodic(tc: TestCase) {
    let mut pit = DeterministicPit::new(TEST_TSC_KHZ);
    let start_tsc = tc.draw(integers::<u64>().min_value(0).max_value(10_000_000));

    // Reload value between 100 and 60000
    let reload = tc.draw(integers::<u16>().min_value(100).max_value(60000));

    // Program channel 0, mode 2, lo/hi byte
    pit.write_port(PIT_PORT_COMMAND, 0b00110100, start_tsc);
    pit.write_port(PIT_PORT_CHANNEL0, (reload & 0xFF) as u8, start_tsc);
    pit.write_port(PIT_PORT_CHANNEL0, (reload >> 8) as u8, start_tsc);

    let tsc_per_period = pit_to_tsc(reload as u64);

    // Check N periods
    let num_periods = tc.draw(integers::<u64>().min_value(1).max_value(20));

    for n in 1..=num_periods {
        let tsc = start_tsc + tsc_per_period * n;

        assert!(
            pit.pending_irq(tsc),
            "period {}: IRQ should be pending at tsc={} (reload={}, period_tsc={})",
            n,
            tsc,
            reload,
            tsc_per_period
        );
        pit.acknowledge_irq();
        assert!(
            !pit.pending_irq(tsc),
            "period {}: IRQ should not be pending after ack",
            n
        );
    }
}

#[hegel::test(test_cases = 200)]
fn mode0_fires_exactly_once(tc: TestCase) {
    let mut pit = DeterministicPit::new(TEST_TSC_KHZ);
    let start_tsc = tc.draw(integers::<u64>().min_value(0).max_value(10_000_000));
    let reload = tc.draw(integers::<u16>().min_value(100).max_value(60000));

    // Program channel 0, mode 0, lo/hi byte
    pit.write_port(PIT_PORT_COMMAND, 0b00110000, start_tsc);
    pit.write_port(PIT_PORT_CHANNEL0, (reload & 0xFF) as u8, start_tsc);
    pit.write_port(PIT_PORT_CHANNEL0, (reload >> 8) as u8, start_tsc);

    let tsc_for_count = pit_to_tsc(reload as u64);

    // Should fire once at terminal count
    assert!(pit.pending_irq(start_tsc + tsc_for_count));
    pit.acknowledge_irq();

    // Should NOT fire a second time (one-shot mode)
    assert!(!pit.pending_irq(start_tsc + tsc_for_count * 2));
    assert!(!pit.pending_irq(start_tsc + tsc_for_count * 3));
}

#[hegel::test(test_cases = 200)]
fn port_61_refresh_toggles(tc: TestCase) {
    let mut pit = DeterministicPit::new(TEST_TSC_KHZ);
    let tsc = tc.draw(integers::<u64>().min_value(0).max_value(10_000_000));

    let n_reads = tc.draw(integers::<usize>().min_value(2).max_value(20));
    let mut prev_bit4 = None;

    for _ in 0..n_reads {
        let val = pit.read_port(PORT_SYSTEM_CONTROL_B, tsc);
        let bit4 = val & 0x10;
        if let Some(prev) = prev_bit4 {
            assert_ne!(bit4, prev, "refresh bit (4) must toggle on each read");
        }
        prev_bit4 = Some(bit4);
    }
}

#[hegel::test(test_cases = 200)]
fn lohibyte_arms_only_after_both_bytes(tc: TestCase) {
    let mut pit = DeterministicPit::new(TEST_TSC_KHZ);
    let tsc = tc.draw(integers::<u64>().min_value(0).max_value(10_000_000));

    // Pick a channel (0, 1, or 2)
    let ch = tc.draw(integers::<u8>().min_value(0).max_value(2));

    // Command byte: channel << 6 | 0x30 (lo/hi access) | 0x04 (mode 2)
    let cmd = (ch << 6) | 0x34;
    let data_port = 0x40 + ch as u16;

    pit.write_port(PIT_PORT_COMMAND, cmd, tsc);

    let lo = tc.draw(integers::<u8>());
    let hi = tc.draw(integers::<u8>().min_value(1));

    // After writing low byte only: not armed
    pit.write_port(data_port, lo, tsc);
    assert!(
        !pit.channel_armed(ch as usize),
        "channel {} should not be armed after low byte only",
        ch
    );

    // After writing high byte: armed
    pit.write_port(data_port, hi, tsc);
    assert!(
        pit.channel_armed(ch as usize),
        "channel {} should be armed after both bytes",
        ch
    );

    let expected_reload = ((hi as u16) << 8) | (lo as u16);
    assert_eq!(
        pit.channel_reload(ch as usize),
        expected_reload,
        "reload should be lo|hi"
    );
}

#[hegel::test(test_cases = 200)]
fn snapshot_restore_preserves_future_irq_behavior(tc: TestCase) {
    let mut pit = DeterministicPit::new(TEST_TSC_KHZ);
    let start_tsc = tc.draw(integers::<u64>().min_value(0).max_value(1_000_000));
    let reload = tc.draw(integers::<u16>().min_value(100).max_value(10000));

    // Program mode 2
    pit.write_port(PIT_PORT_COMMAND, 0b00110100, start_tsc);
    pit.write_port(PIT_PORT_CHANNEL0, (reload & 0xFF) as u8, start_tsc);
    pit.write_port(PIT_PORT_CHANNEL0, (reload >> 8) as u8, start_tsc);

    // Deliver some IRQs
    let pre_irqs = tc.draw(integers::<u64>().min_value(0).max_value(5));
    let tsc_per_period = pit_to_tsc(reload as u64);
    for i in 1..=pre_irqs {
        let tsc = start_tsc + tsc_per_period * i;
        if pit.pending_irq(tsc) {
            pit.acknowledge_irq();
        }
    }

    // Snapshot
    let snap = pit.snapshot();

    // Continue original: check next few IRQs
    let mut orig_irq_tscs = Vec::new();
    for i in (pre_irqs + 1)..=(pre_irqs + 5) {
        let tsc = start_tsc + tsc_per_period * i;
        orig_irq_tscs.push(pit.pending_irq(tsc));
        if pit.pending_irq(tsc) {
            pit.acknowledge_irq();
        }
    }

    // Restore and check same pattern
    let mut restored = DeterministicPit::restore(&snap);
    let mut restored_irq_tscs = Vec::new();
    for i in (pre_irqs + 1)..=(pre_irqs + 5) {
        let tsc = start_tsc + tsc_per_period * i;
        restored_irq_tscs.push(restored.pending_irq(tsc));
        if restored.pending_irq(tsc) {
            restored.acknowledge_irq();
        }
    }

    assert_eq!(
        orig_irq_tscs, restored_irq_tscs,
        "restored PIT must produce same IRQ pattern"
    );
}
