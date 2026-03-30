//! Property-based tests for VcpuScheduler.
//!
//! Key properties:
//! - Same seed → identical schedule trace (determinism).
//! - Snapshot/restore at any point → identical future trace (reproducibility).
//! - Active vCPU index always < num_vcpus (bounds).
//! - Single-vCPU scheduler never switches.
//! - Quantum is always in configured range for Randomized strategy.
//! - Round-robin visits all vCPUs equally over time.

use chaoscontrol_vmm::scheduler::{SchedulerConfig, SchedulingStrategy, VcpuScheduler};
use hegel::generators::*;
use hegel::TestCase;

fn vcpu_counts() -> impl hegel::Generator<usize> {
    integers::<usize>().min_value(1).max_value(16)
}

fn quanta() -> impl hegel::Generator<u64> {
    integers::<u64>().min_value(1).max_value(500)
}

fn seeds() -> impl hegel::Generator<u64> {
    integers::<u64>()
}

fn tick_counts() -> impl hegel::Generator<usize> {
    integers::<usize>().min_value(10).max_value(2000)
}

/// Run a scheduler for N ticks, collecting a trace of (active_vcpu, remaining) at each switch.
fn run_trace(sched: &mut VcpuScheduler, ticks: usize) -> Vec<(usize, u64)> {
    let mut trace = Vec::new();
    for _ in 0..ticks {
        if sched.tick() {
            sched.advance();
            trace.push((sched.active(), sched.remaining()));
        }
    }
    trace
}

#[hegel::test(test_cases = 300)]
fn deterministic_same_seed_same_trace(tc: TestCase) {
    let num_vcpus = tc.draw(vcpu_counts());
    let quantum = tc.draw(quanta());
    let seed = tc.draw(seeds());
    let ticks = tc.draw(tick_counts());

    let min_q = tc.draw(integers::<u64>().min_value(1).max_value(50));
    let max_q = tc.draw(integers::<u64>().min_value(min_q + 1).max_value(500));

    let config = SchedulerConfig {
        num_vcpus,
        quantum,
        strategy: SchedulingStrategy::Randomized {
            min_quantum: min_q,
            max_quantum: max_q,
        },
        seed,
    };

    let mut s1 = VcpuScheduler::new(&config);
    let mut s2 = VcpuScheduler::new(&config);

    let t1 = run_trace(&mut s1, ticks);
    let t2 = run_trace(&mut s2, ticks);

    assert_eq!(t1, t2, "same config must produce same trace");
}

#[hegel::test(test_cases = 300)]
fn snapshot_restore_produces_same_future(tc: TestCase) {
    let num_vcpus = tc.draw(integers::<usize>().min_value(2).max_value(8));
    let quantum = tc.draw(quanta());
    let seed = tc.draw(seeds());

    let pre_ticks = tc.draw(integers::<usize>().min_value(1).max_value(500));
    let post_ticks = tc.draw(integers::<usize>().min_value(10).max_value(500));

    let config = SchedulerConfig {
        num_vcpus,
        quantum,
        strategy: SchedulingStrategy::Randomized {
            min_quantum: 5,
            max_quantum: 100,
        },
        seed,
    };

    let mut original = VcpuScheduler::new(&config);

    // Advance to some mid-point
    for _ in 0..pre_ticks {
        if original.tick() {
            original.advance();
        }
    }

    let snap = original.snapshot();

    // Continue original
    let orig_trace = run_trace(&mut original, post_ticks);

    // Restore and replay
    let mut restored = VcpuScheduler::new(&config);
    restored.restore(&snap);
    let restored_trace = run_trace(&mut restored, post_ticks);

    assert_eq!(orig_trace, restored_trace);
}

#[hegel::test(test_cases = 300)]
fn active_vcpu_always_in_bounds(tc: TestCase) {
    let num_vcpus = tc.draw(vcpu_counts());
    let quantum = tc.draw(quanta());
    let seed = tc.draw(seeds());
    let ticks = tc.draw(tick_counts());

    let config = SchedulerConfig {
        num_vcpus,
        quantum,
        strategy: SchedulingStrategy::RoundRobin,
        seed,
    };

    let mut sched = VcpuScheduler::new(&config);
    for _ in 0..ticks {
        assert!(
            sched.active() < num_vcpus,
            "active={} but num_vcpus={}",
            sched.active(),
            num_vcpus
        );
        if sched.tick() {
            sched.advance();
        }
    }
}

#[hegel::test(test_cases = 300)]
fn single_vcpu_never_switches(tc: TestCase) {
    let quantum = tc.draw(quanta());
    let seed = tc.draw(seeds());
    let ticks = tc.draw(tick_counts());

    let config = SchedulerConfig {
        num_vcpus: 1,
        quantum,
        strategy: SchedulingStrategy::RoundRobin,
        seed,
    };

    let mut sched = VcpuScheduler::new(&config);
    for _ in 0..ticks {
        assert!(!sched.tick(), "single vCPU must never switch");
        assert_eq!(sched.active(), 0);
    }
}

#[hegel::test(test_cases = 300)]
fn randomized_quantum_in_range(tc: TestCase) {
    let num_vcpus = tc.draw(integers::<usize>().min_value(2).max_value(8));
    let min_q = tc.draw(integers::<u64>().min_value(1).max_value(50));
    let max_q = tc.draw(integers::<u64>().min_value(min_q + 1).max_value(500));
    let seed = tc.draw(seeds());
    let ticks = tc.draw(tick_counts());

    let config = SchedulerConfig {
        num_vcpus,
        quantum: 10, // initial quantum
        strategy: SchedulingStrategy::Randomized {
            min_quantum: min_q,
            max_quantum: max_q,
        },
        seed,
    };

    let mut sched = VcpuScheduler::new(&config);
    for _ in 0..ticks {
        if sched.tick() {
            sched.advance();
            let r = sched.remaining();
            assert!(
                r >= min_q && r < max_q,
                "quantum {} not in [{}, {})",
                r,
                min_q,
                max_q
            );
        }
    }
}

#[hegel::test(test_cases = 200)]
fn round_robin_visits_all_vcpus(tc: TestCase) {
    let num_vcpus = tc.draw(integers::<usize>().min_value(2).max_value(8));
    let quantum = tc.draw(integers::<u64>().min_value(1).max_value(20));

    let config = SchedulerConfig {
        num_vcpus,
        quantum,
        strategy: SchedulingStrategy::RoundRobin,
        ..Default::default()
    };

    let mut sched = VcpuScheduler::new(&config);
    let mut seen = vec![false; num_vcpus];

    // Run enough ticks to cycle through all vCPUs at least once
    let total_ticks = (num_vcpus as u64 * quantum * 2) as usize;
    for _ in 0..total_ticks {
        seen[sched.active()] = true;
        if sched.tick() {
            sched.advance();
        }
    }

    for (i, &s) in seen.iter().enumerate() {
        assert!(s, "vCPU {} was never scheduled in {} ticks", i, total_ticks);
    }
}

#[hegel::test(test_cases = 200)]
fn set_active_does_not_affect_rng(tc: TestCase) {
    let num_vcpus = tc.draw(integers::<usize>().min_value(2).max_value(8));
    let seed = tc.draw(seeds());
    let pre_ticks = tc.draw(integers::<usize>().min_value(1).max_value(200));
    let post_ticks = tc.draw(integers::<usize>().min_value(10).max_value(500));
    let forced_vcpu = tc.draw(integers::<usize>().min_value(0).max_value(num_vcpus - 1));

    let config = SchedulerConfig {
        num_vcpus,
        quantum: 10,
        strategy: SchedulingStrategy::Randomized {
            min_quantum: 5,
            max_quantum: 50,
        },
        seed,
    };

    // Run A: normal scheduling
    let mut a = VcpuScheduler::new(&config);
    for _ in 0..pre_ticks {
        if a.tick() {
            a.advance();
        }
    }
    let snap = a.snapshot();

    // Run B: restore then call set_active (which shouldn't touch RNG)
    let mut b = VcpuScheduler::new(&config);
    b.restore(&snap);
    b.set_active(forced_vcpu);

    // After set_active, the remaining for b is reset to quantum (10),
    // so we can't compare traces directly. But the RNG state should be
    // identical — the next advance() after quantum exhaustion should
    // produce the same quantum value.

    // Exhaust b's current quantum
    for _ in 0..b.remaining() {
        if b.tick() {
            b.advance();
            break;
        }
    }

    // Exhaust a's current quantum
    for _ in 0..a.remaining() {
        if a.tick() {
            a.advance();
            break;
        }
    }

    // Now both have done one advance() consuming one RNG token.
    // From here they should produce identical quanta.
    let trace_a = run_trace(&mut a, post_ticks);
    let trace_b = run_trace(&mut b, post_ticks);

    // We can't assert trace equality because active vCPU differs.
    // But the quantum values (second element of tuples) should match
    // starting from the first advance.
    let quanta_a: Vec<u64> = trace_a.iter().map(|(_, q)| *q).collect();
    let quanta_b: Vec<u64> = trace_b.iter().map(|(_, q)| *q).collect();
    assert_eq!(quanta_a, quanta_b, "set_active must not consume RNG state");
}
