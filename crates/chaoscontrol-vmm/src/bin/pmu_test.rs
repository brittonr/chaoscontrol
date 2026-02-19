//! Quick test: does PMU overflow deliver SIGIO?
use std::sync::atomic::{AtomicU64, Ordering};

static SIGIO_COUNT: AtomicU64 = AtomicU64::new(0);

extern "C" fn sigio_handler(_sig: libc::c_int) {
    SIGIO_COUNT.fetch_add(1, Ordering::Relaxed);
}

fn main() {
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = sigio_handler as *const () as usize;
        sa.sa_flags = 0;
        libc::sigaction(libc::SIGIO, &sa, std::ptr::null_mut());
    }

    let period: u64 = 100_000;

    // Test without exclude_host first
    use chaoscontrol_vmm::perf::InstructionCounter;

    println!("=== Testing PMU overflow (period={}) ===", period);

    match InstructionCounter::with_overflow(period) {
        Ok(counter) => {
            SIGIO_COUNT.store(0, Ordering::Relaxed);
            counter.reset_and_enable();

            // Busy loop
            let mut x: u64 = 0;
            for i in 0..10_000_000u64 {
                x = x.wrapping_add(i);
            }

            counter.disable();
            let sigs = SIGIO_COUNT.load(Ordering::Relaxed);
            let count = counter.read();
            println!("SIGIO count: {} (expected ~{})", sigs, 10_000_000 / period);
            println!("Counter value: {} (x={})", count, x);
            if sigs == 0 {
                println!("❌ No SIGIO delivered — overflow not working!");
            } else {
                println!("✅ SIGIO working");
            }
        }
        Err(e) => println!("PMU not available: {}", e),
    }

    // Also test with resume/disable gating
    match InstructionCounter::with_overflow(period) {
        Ok(counter) => {
            SIGIO_COUNT.store(0, Ordering::Relaxed);
            counter.reset_and_enable();
            counter.disable(); // Pause

            // Resume, do work, disable — like vcpu.run() gating
            for _ in 0..10 {
                counter.resume();
                let mut x: u64 = 0;
                for i in 0..500_000u64 {
                    x = x.wrapping_add(i);
                }
                counter.disable();
                let _ = x;
            }

            let sigs = SIGIO_COUNT.load(Ordering::Relaxed);
            let count = counter.read();
            println!("\n=== Gated mode (resume/disable per iteration) ===");
            println!("SIGIO count: {} (expected ~{})", sigs, 5_000_000 / period);
            println!("Counter value: {}", count);
            if sigs == 0 {
                println!("❌ No SIGIO in gated mode!");
            } else {
                println!("✅ Gated mode working");
            }
        }
        Err(e) => println!("PMU not available: {}", e),
    }
}
