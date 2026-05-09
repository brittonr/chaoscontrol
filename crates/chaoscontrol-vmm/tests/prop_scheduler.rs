//! Deterministic sweep tests for VcpuScheduler.
//!
//! These preserve the prior property-test invariants without pulling a proc-macro
//! property-test dependency into the dependency-audit surface.

use chaoscontrol_vmm::scheduler::{SchedulerConfig, SchedulingStrategy, VcpuScheduler};

const WIDE_CASES: u64 = 300;
const NORMAL_CASES: u64 = 200;

#[derive(Clone)]
struct DeterministicCase {
    state: u64,
}

impl DeterministicCase {
    fn new(index: u64) -> Self {
        Self {
            state: index ^ 0xa24b_aed4_963e_e407,
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

    fn usize(&mut self, min: usize, max: usize) -> usize {
        min + (self.next() as usize % (max - min + 1))
    }
}

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

#[test]
fn deterministic_same_seed_same_trace() {
    for case in 0..WIDE_CASES {
        let mut tc = DeterministicCase::new(case);
        let num_vcpus = tc.usize(1, 16);
        let quantum = tc.u64(1, 500);
        let seed = tc.next();
        let ticks = tc.usize(10, 2000);
        let min_q = tc.u64(1, 50);
        let max_q = tc.u64(min_q + 1, 500);

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

        assert_eq!(
            run_trace(&mut s1, ticks),
            run_trace(&mut s2, ticks),
            "case {case}: same config must produce same trace"
        );
    }
}

#[test]
fn snapshot_restore_produces_same_future() {
    for case in 0..WIDE_CASES {
        let mut tc = DeterministicCase::new(case);
        let num_vcpus = tc.usize(2, 8);
        let quantum = tc.u64(1, 500);
        let seed = tc.next();
        let pre_ticks = tc.usize(1, 500);
        let post_ticks = tc.usize(10, 500);

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
        for _ in 0..pre_ticks {
            if original.tick() {
                original.advance();
            }
        }

        let snap = original.snapshot();
        let orig_trace = run_trace(&mut original, post_ticks);

        let mut restored = VcpuScheduler::new(&config);
        restored.restore(&snap);
        let restored_trace = run_trace(&mut restored, post_ticks);

        assert_eq!(orig_trace, restored_trace, "case {case}");
    }
}

#[test]
fn active_vcpu_always_in_bounds() {
    for case in 0..WIDE_CASES {
        let mut tc = DeterministicCase::new(case);
        let num_vcpus = tc.usize(1, 16);
        let quantum = tc.u64(1, 500);
        let seed = tc.next();
        let ticks = tc.usize(10, 2000);

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
                "case {case}: active={} but num_vcpus={num_vcpus}",
                sched.active()
            );
            if sched.tick() {
                sched.advance();
            }
        }
    }
}

#[test]
fn single_vcpu_never_switches() {
    for case in 0..WIDE_CASES {
        let mut tc = DeterministicCase::new(case);
        let quantum = tc.u64(1, 500);
        let seed = tc.next();
        let ticks = tc.usize(10, 2000);

        let config = SchedulerConfig {
            num_vcpus: 1,
            quantum,
            strategy: SchedulingStrategy::RoundRobin,
            seed,
        };

        let mut sched = VcpuScheduler::new(&config);
        for _ in 0..ticks {
            assert!(!sched.tick(), "case {case}: single vCPU must never switch");
            assert_eq!(sched.active(), 0);
        }
    }
}

#[test]
fn randomized_quantum_in_range() {
    for case in 0..WIDE_CASES {
        let mut tc = DeterministicCase::new(case);
        let num_vcpus = tc.usize(2, 8);
        let min_q = tc.u64(1, 50);
        let max_q = tc.u64(min_q + 1, 500);
        let seed = tc.next();
        let ticks = tc.usize(10, 2000);

        let config = SchedulerConfig {
            num_vcpus,
            quantum: 10,
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
                let remaining = sched.remaining();
                assert!(
                    remaining >= min_q && remaining < max_q,
                    "case {case}: quantum {remaining} not in [{min_q}, {max_q})"
                );
            }
        }
    }
}

#[test]
fn round_robin_visits_all_vcpus() {
    for case in 0..NORMAL_CASES {
        let mut tc = DeterministicCase::new(case);
        let num_vcpus = tc.usize(2, 8);
        let quantum = tc.u64(1, 20);

        let config = SchedulerConfig {
            num_vcpus,
            quantum,
            strategy: SchedulingStrategy::RoundRobin,
            ..Default::default()
        };

        let mut sched = VcpuScheduler::new(&config);
        let mut seen = vec![false; num_vcpus];

        let total_ticks = (num_vcpus as u64 * quantum * 2) as usize;
        for _ in 0..total_ticks {
            seen[sched.active()] = true;
            if sched.tick() {
                sched.advance();
            }
        }

        for (i, seen_vcpu) in seen.iter().enumerate() {
            assert!(
                *seen_vcpu,
                "case {case}: vCPU {i} was never scheduled in {total_ticks} ticks"
            );
        }
    }
}

#[test]
fn set_active_does_not_affect_rng() {
    for case in 0..NORMAL_CASES {
        let mut tc = DeterministicCase::new(case);
        let num_vcpus = tc.usize(2, 8);
        let seed = tc.next();
        let pre_ticks = tc.usize(1, 200);
        let post_ticks = tc.usize(10, 500);
        let forced_vcpu = tc.usize(0, num_vcpus - 1);

        let config = SchedulerConfig {
            num_vcpus,
            quantum: 10,
            strategy: SchedulingStrategy::Randomized {
                min_quantum: 5,
                max_quantum: 50,
            },
            seed,
        };

        let mut a = VcpuScheduler::new(&config);
        for _ in 0..pre_ticks {
            if a.tick() {
                a.advance();
            }
        }
        let snap = a.snapshot();

        let mut b = VcpuScheduler::new(&config);
        b.restore(&snap);
        b.set_active(forced_vcpu);

        for _ in 0..b.remaining() {
            if b.tick() {
                b.advance();
                break;
            }
        }

        for _ in 0..a.remaining() {
            if a.tick() {
                a.advance();
                break;
            }
        }

        let quanta_a: Vec<u64> = run_trace(&mut a, post_ticks)
            .iter()
            .map(|(_, quantum)| *quantum)
            .collect();
        let quanta_b: Vec<u64> = run_trace(&mut b, post_ticks)
            .iter()
            .map(|(_, quantum)| *quantum)
            .collect();
        assert_eq!(quanta_a, quanta_b, "case {case}: set_active consumed RNG");
    }
}
