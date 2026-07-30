//! Manual PMU overflow-attribution smoke test.

use chaoscontrol_vmm::perf::InstructionCounter;

const OVERFLOW_PERIOD: u64 = 100_000;
const BUSY_ITERATIONS: u64 = 10_000_000;
const GATED_ITERATIONS: usize = 10;
const GATED_BUSY_ITERATIONS: u64 = 500_000;

fn main() {
    println!("=== Testing attributed PMU overflow (period={OVERFLOW_PERIOD}) ===");
    match InstructionCounter::with_overflow(OVERFLOW_PERIOD) {
        Ok(counter) => {
            let generation = counter
                .overflow_generation()
                .expect("read overflow generation");
            counter.reset_and_enable().expect("enable PMU counter");
            let mut value = 0u64;
            for index in 0..BUSY_ITERATIONS {
                value = value.wrapping_add(index);
            }
            counter.disable().expect("disable PMU counter");
            let count = counter.read().expect("read PMU counter");
            let overflowed = counter
                .overflow_since(generation)
                .expect("read overflow attribution");
            println!("overflow attributed: {overflowed}");
            println!("counter value: {count} (value={value})");
        }
        Err(error) => println!("PMU not available: {error}"),
    }

    match InstructionCounter::with_overflow(OVERFLOW_PERIOD) {
        Ok(counter) => {
            let generation = counter
                .overflow_generation()
                .expect("read overflow generation");
            counter.reset_and_enable().expect("enable PMU counter");
            counter.disable().expect("pause PMU counter");
            for _ in 0..GATED_ITERATIONS {
                counter.resume().expect("resume PMU counter");
                let mut value = 0u64;
                for index in 0..GATED_BUSY_ITERATIONS {
                    value = value.wrapping_add(index);
                }
                counter.disable().expect("pause PMU counter");
                std::hint::black_box(value);
            }
            let count = counter.read().expect("read PMU counter");
            let overflowed = counter
                .overflow_since(generation)
                .expect("read overflow attribution");
            println!("\n=== Gated mode ===");
            println!("overflow attributed: {overflowed}");
            println!("counter value: {count}");
        }
        Err(error) => println!("PMU not available: {error}"),
    }
}
