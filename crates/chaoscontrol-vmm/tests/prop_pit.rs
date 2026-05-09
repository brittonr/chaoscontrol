//! Deterministic sweep tests for DeterministicPit.
//!
//! These preserve the prior property-test invariants without pulling a proc-macro
//! property-test dependency into the dependency-audit surface.

use chaoscontrol_vmm::devices::pit::{
    DeterministicPit, PIT_FREQ_HZ, PIT_PORT_CHANNEL0, PIT_PORT_COMMAND, PORT_SYSTEM_CONTROL_B,
};

const CASES: u64 = 200;
const TEST_TSC_KHZ: u32 = 2_400_000;

#[derive(Clone)]
struct DeterministicCase {
    state: u64,
}

impl DeterministicCase {
    fn new(index: u64) -> Self {
        Self {
            state: index ^ 0x4f1b_bcdc_b5aa_5d31,
        }
    }

    fn next(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.state
    }

    fn u64(&mut self, min: u64, max: u64) -> u64 {
        min + (self.next() % (max - min + 1))
    }

    fn u16(&mut self, min: u16, max: u16) -> u16 {
        min + (self.next() as u16 % (max - min + 1))
    }

    fn u8(&mut self, min: u8, max: u8) -> u8 {
        min + (self.next() as u8 % (max - min + 1))
    }

    fn any_u8(&mut self) -> u8 {
        self.next() as u8
    }
}

fn pit_to_tsc(pit_ticks: u64) -> u64 {
    (pit_ticks as u128 * TEST_TSC_KHZ as u128 * 1000).div_ceil(PIT_FREQ_HZ as u128) as u64
}

#[test]
fn snapshot_restore_round_trip() {
    for case in 0..CASES {
        let mut tc = DeterministicCase::new(case);
        let mut pit = DeterministicPit::new(TEST_TSC_KHZ);
        let start_tsc = tc.u64(0, 100_000_000);

        pit.write_port(PIT_PORT_COMMAND, 0b00110100, start_tsc);
        let reload_lo = tc.any_u8();
        let reload_hi = tc.u8(1, u8::MAX);
        pit.write_port(PIT_PORT_CHANNEL0, reload_lo, start_tsc);
        pit.write_port(PIT_PORT_CHANNEL0, reload_hi, start_tsc);

        let port_61_val = tc.u8(0, 3);
        pit.write_port(PORT_SYSTEM_CONTROL_B, port_61_val, start_tsc);

        let irq_count = tc.u64(0, 50);
        for _ in 0..irq_count {
            pit.acknowledge_irq();
        }

        let snap1 = pit.snapshot();
        let restored = DeterministicPit::restore(&snap1);
        let snap2 = restored.snapshot();

        assert_eq!(
            snap1, snap2,
            "case {case}: snapshot round-trip must be lossless"
        );
    }
}

#[test]
fn mode2_irqs_are_periodic() {
    for case in 0..CASES {
        let mut tc = DeterministicCase::new(case);
        let mut pit = DeterministicPit::new(TEST_TSC_KHZ);
        let start_tsc = tc.u64(0, 10_000_000);
        let reload = tc.u16(100, 60_000);

        pit.write_port(PIT_PORT_COMMAND, 0b00110100, start_tsc);
        pit.write_port(PIT_PORT_CHANNEL0, (reload & 0xFF) as u8, start_tsc);
        pit.write_port(PIT_PORT_CHANNEL0, (reload >> 8) as u8, start_tsc);

        let tsc_per_period = pit_to_tsc(reload as u64);
        let num_periods = tc.u64(1, 20);

        for n in 1..=num_periods {
            let tsc = start_tsc + tsc_per_period * n;

            assert!(
                pit.pending_irq(tsc),
                "case {case}, period {n}: IRQ should be pending at tsc={tsc}"
            );
            pit.acknowledge_irq();
            assert!(
                !pit.pending_irq(tsc),
                "case {case}, period {n}: IRQ should not be pending after ack"
            );
        }
    }
}

#[test]
fn mode0_fires_exactly_once() {
    for case in 0..CASES {
        let mut tc = DeterministicCase::new(case);
        let mut pit = DeterministicPit::new(TEST_TSC_KHZ);
        let start_tsc = tc.u64(0, 10_000_000);
        let reload = tc.u16(100, 60_000);

        pit.write_port(PIT_PORT_COMMAND, 0b00110000, start_tsc);
        pit.write_port(PIT_PORT_CHANNEL0, (reload & 0xFF) as u8, start_tsc);
        pit.write_port(PIT_PORT_CHANNEL0, (reload >> 8) as u8, start_tsc);

        let tsc_for_count = pit_to_tsc(reload as u64);

        assert!(pit.pending_irq(start_tsc + tsc_for_count), "case {case}");
        pit.acknowledge_irq();

        assert!(
            !pit.pending_irq(start_tsc + tsc_for_count * 2),
            "case {case}"
        );
        assert!(
            !pit.pending_irq(start_tsc + tsc_for_count * 3),
            "case {case}"
        );
    }
}

#[test]
fn port_61_refresh_toggles() {
    for case in 0..CASES {
        let mut tc = DeterministicCase::new(case);
        let mut pit = DeterministicPit::new(TEST_TSC_KHZ);
        let tsc = tc.u64(0, 10_000_000);
        let n_reads = tc.u64(2, 20);
        let mut prev_bit4 = None;

        for _ in 0..n_reads {
            let val = pit.read_port(PORT_SYSTEM_CONTROL_B, tsc);
            let bit4 = val & 0x10;
            if let Some(prev) = prev_bit4 {
                assert_ne!(bit4, prev, "case {case}: refresh bit must toggle");
            }
            prev_bit4 = Some(bit4);
        }
    }
}

#[test]
fn lohibyte_arms_only_after_both_bytes() {
    for case in 0..CASES {
        let mut tc = DeterministicCase::new(case);
        let mut pit = DeterministicPit::new(TEST_TSC_KHZ);
        let tsc = tc.u64(0, 10_000_000);
        let channel = tc.u8(0, 2);

        let cmd = (channel << 6) | 0x34;
        let data_port = 0x40 + channel as u16;

        pit.write_port(PIT_PORT_COMMAND, cmd, tsc);

        let lo = tc.any_u8();
        let hi = tc.u8(1, u8::MAX);

        pit.write_port(data_port, lo, tsc);
        assert!(
            !pit.channel_armed(channel as usize),
            "case {case}: channel {channel} should not be armed after low byte only"
        );

        pit.write_port(data_port, hi, tsc);
        assert!(
            pit.channel_armed(channel as usize),
            "case {case}: channel {channel} should be armed after both bytes"
        );

        let expected_reload = ((hi as u16) << 8) | (lo as u16);
        assert_eq!(
            pit.channel_reload(channel as usize),
            expected_reload,
            "case {case}: reload should be lo|hi"
        );
    }
}

#[test]
fn snapshot_restore_preserves_future_irq_behavior() {
    for case in 0..CASES {
        let mut tc = DeterministicCase::new(case);
        let mut pit = DeterministicPit::new(TEST_TSC_KHZ);
        let start_tsc = tc.u64(0, 1_000_000);
        let reload = tc.u16(100, 10_000);

        pit.write_port(PIT_PORT_COMMAND, 0b00110100, start_tsc);
        pit.write_port(PIT_PORT_CHANNEL0, (reload & 0xFF) as u8, start_tsc);
        pit.write_port(PIT_PORT_CHANNEL0, (reload >> 8) as u8, start_tsc);

        let pre_irqs = tc.u64(0, 5);
        let tsc_per_period = pit_to_tsc(reload as u64);
        for i in 1..=pre_irqs {
            let tsc = start_tsc + tsc_per_period * i;
            if pit.pending_irq(tsc) {
                pit.acknowledge_irq();
            }
        }

        let snap = pit.snapshot();

        let mut orig_irq_tscs = Vec::new();
        for i in (pre_irqs + 1)..=(pre_irqs + 5) {
            let tsc = start_tsc + tsc_per_period * i;
            orig_irq_tscs.push(pit.pending_irq(tsc));
            if pit.pending_irq(tsc) {
                pit.acknowledge_irq();
            }
        }

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
            "case {case}: restored PIT must produce same IRQ pattern"
        );
    }
}
