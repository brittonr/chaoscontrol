//! Deterministic vCPU scheduler for SMP virtual machines.
//!
//! When a VM has multiple vCPUs, only one runs at a time (serialized
//! execution, Antithesis-style). The [`VcpuScheduler`] decides which
//! vCPU runs and for how many exits before switching.
//!
//! # Scheduling model
//!
//! The scheduler assigns each vCPU a **quantum** — the number of VM exits
//! it may execute before the next vCPU gets a turn. The quantum can be:
//!
//! - **Fixed**: every vCPU gets the same number of exits (round-robin).
//! - **Randomized**: each quantum is drawn from a seeded PRNG, exploring
//!   different interleaving patterns across runs.
//!
//! Because the scheduler is seeded, the exact sequence of (vCPU, quantum)
//! pairs is deterministic for a given seed, making execution reproducible.
//!
//! # Single-vCPU mode
//!
//! When `num_vcpus == 1`, the scheduler is a no-op: it always returns
//! vCPU 0 with no switching overhead.

use rand::RngCore;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

/// Scheduling strategy for multi-vCPU VMs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulingStrategy {
    /// Fixed round-robin: each vCPU gets exactly `quantum` exits.
    RoundRobin,
    /// Randomized quantum: each turn draws a random quantum in
    /// `[min_quantum, max_quantum]` from the seeded PRNG.
    Randomized {
        /// Minimum exits per vCPU turn.
        min_quantum: u64,
        /// Maximum exits per vCPU turn.
        max_quantum: u64,
    },
}

/// Configuration for the vCPU scheduler.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Number of vCPUs to schedule.
    pub num_vcpus: usize,
    /// Default quantum (exits per vCPU turn) for RoundRobin mode.
    pub quantum: u64,
    /// Scheduling strategy.
    pub strategy: SchedulingStrategy,
    /// Seed for the scheduling PRNG.
    pub seed: u64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            num_vcpus: 1,
            quantum: 100,
            strategy: SchedulingStrategy::RoundRobin,
            seed: 0,
        }
    }
}

/// Deterministic vCPU scheduler.
///
/// Tracks the current vCPU, remaining exits in its quantum, and the
/// scheduling state needed for round-robin or randomized switching.
#[derive(Debug, Clone)]
pub struct VcpuScheduler {
    /// Number of vCPUs.
    num_vcpus: usize,
    /// Index of the currently active vCPU.
    active: usize,
    /// Exits remaining in the current vCPU's quantum.
    remaining: u64,
    /// Default quantum for RoundRobin.
    quantum: u64,
    /// Strategy.
    strategy: SchedulingStrategy,
    /// PRNG for randomized scheduling.
    rng: ChaCha20Rng,
}

impl VcpuScheduler {
    /// Create a new scheduler from configuration.
    pub fn new(config: &SchedulerConfig) -> Self {
        let mut rng_key = [0u8; 32];
        // Domain-separated seed for scheduler RNG
        let derived = config.seed.wrapping_add(0x5343_4845_4430); // "SCHED0"
        rng_key[..8].copy_from_slice(&derived.to_le_bytes());
        let rng = ChaCha20Rng::from_seed(rng_key);

        let quantum = config.quantum.max(1);
        Self {
            num_vcpus: config.num_vcpus.max(1),
            active: 0,
            remaining: quantum,
            quantum,
            strategy: config.strategy,
            rng,
        }
    }

    /// Get the currently active vCPU index.
    #[inline]
    pub fn active(&self) -> usize {
        self.active
    }

    /// Get the number of vCPUs.
    #[inline]
    pub fn num_vcpus(&self) -> usize {
        self.num_vcpus
    }

    /// Called after each VM exit. Returns `true` if the active vCPU
    /// should switch (quantum exhausted).
    ///
    /// When `true` is returned, the caller should call [`advance`](Self::advance)
    /// to move to the next vCPU.
    #[inline]
    pub fn tick(&mut self) -> bool {
        if self.num_vcpus <= 1 {
            return false; // Single vCPU — never switch
        }
        self.remaining = self.remaining.saturating_sub(1);
        self.remaining == 0
    }

    /// Advance to the next vCPU in the schedule.
    ///
    /// Returns the new active vCPU index.
    pub fn advance(&mut self) -> usize {
        if self.num_vcpus <= 1 {
            return 0;
        }

        // Move to next vCPU (round-robin)
        self.active = (self.active + 1) % self.num_vcpus;

        // Assign quantum for the new turn
        self.remaining = match self.strategy {
            SchedulingStrategy::RoundRobin => self.quantum,
            SchedulingStrategy::Randomized {
                min_quantum,
                max_quantum,
            } => {
                let range = max_quantum.saturating_sub(min_quantum).max(1);
                let random = self.rng.next_u64() % range;
                min_quantum + random
            }
        };

        self.active
    }

    /// Reset the scheduler to its initial state (vCPU 0, full quantum).
    pub fn reset(&mut self) {
        self.active = 0;
        self.remaining = self.quantum;
    }

    /// Force the scheduler to a specific vCPU with a fresh quantum.
    ///
    /// Used by the SIGALRM preemption handler to switch vCPUs outside
    /// the normal quantum-based scheduling. Does NOT consume RNG state
    /// to avoid non-determinism from wall-clock timer jitter.
    pub fn set_active(&mut self, vcpu: usize) {
        debug_assert!(vcpu < self.num_vcpus, "vCPU index out of bounds");
        self.active = vcpu;
        self.remaining = self.quantum;
    }

    /// Get remaining exits in the current quantum.
    #[inline]
    pub fn remaining(&self) -> u64 {
        self.remaining
    }

    /// Produce a serialisable snapshot.
    pub fn snapshot(&self) -> SchedulerSnapshot {
        SchedulerSnapshot {
            active: self.active,
            remaining: self.remaining,
            rng_seed: self.rng.get_seed(),
            rng_word_pos: self.rng.get_word_pos(),
        }
    }

    /// Restore from a snapshot, keeping the config (strategy, quantum, num_vcpus).
    pub fn restore(&mut self, snap: &SchedulerSnapshot) {
        self.active = snap.active;
        self.remaining = snap.remaining;
        self.rng = ChaCha20Rng::from_seed(snap.rng_seed);
        self.rng.set_word_pos(snap.rng_word_pos);
    }
}

/// Serialisable scheduler state.
#[derive(Debug, Clone)]
pub struct SchedulerSnapshot {
    /// Active vCPU index.
    pub active: usize,
    /// Remaining exits in current quantum.
    pub remaining: u64,
    /// ChaCha20 RNG seed.
    pub rng_seed: [u8; 32],
    /// ChaCha20 RNG stream position (word offset).
    pub rng_word_pos: u128,
}

// ═══════════════════════════════════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_vcpu_never_switches() {
        let config = SchedulerConfig {
            num_vcpus: 1,
            quantum: 10,
            ..Default::default()
        };
        let mut sched = VcpuScheduler::new(&config);

        // 100 ticks — should never switch
        for _ in 0..100 {
            assert!(!sched.tick());
            assert_eq!(sched.active(), 0);
        }
    }

    #[test]
    fn round_robin_switches_after_quantum() {
        let config = SchedulerConfig {
            num_vcpus: 3,
            quantum: 5,
            strategy: SchedulingStrategy::RoundRobin,
            ..Default::default()
        };
        let mut sched = VcpuScheduler::new(&config);
        assert_eq!(sched.active(), 0);

        // Tick 5 times — should switch
        for i in 0..4 {
            assert!(!sched.tick(), "tick {} should not switch", i);
        }
        assert!(sched.tick(), "tick 4 (5th) should switch");

        let next = sched.advance();
        assert_eq!(next, 1);
        assert_eq!(sched.active(), 1);

        // Another 5 ticks → switch to vCPU 2
        for _ in 0..4 {
            assert!(!sched.tick());
        }
        assert!(sched.tick());
        let next = sched.advance();
        assert_eq!(next, 2);

        // Another 5 → wrap to vCPU 0
        for _ in 0..4 {
            assert!(!sched.tick());
        }
        assert!(sched.tick());
        let next = sched.advance();
        assert_eq!(next, 0);
    }

    #[test]
    fn randomized_produces_variable_quanta() {
        let config = SchedulerConfig {
            num_vcpus: 2,
            strategy: SchedulingStrategy::Randomized {
                min_quantum: 10,
                max_quantum: 100,
            },
            seed: 42,
            ..Default::default()
        };
        let mut sched = VcpuScheduler::new(&config);

        // Exhaust first quantum (fixed at config.quantum for initial turn)
        let first_remaining = sched.remaining();
        for _ in 0..first_remaining {
            if sched.tick() {
                sched.advance();
            }
        }

        // Collect quanta for several turns
        let mut quanta = Vec::new();
        for _ in 0..20 {
            quanta.push(sched.remaining());
            for _ in 0..sched.remaining() {
                if sched.tick() {
                    sched.advance();
                    break;
                }
            }
        }

        // Should have variation (not all the same)
        let all_same = quanta.iter().all(|&q| q == quanta[0]);
        assert!(!all_same, "Randomized quanta should vary: {:?}", quanta);

        // All in range
        for &q in &quanta {
            assert!(q >= 10 && q < 100, "Quantum {} out of range [10, 100)", q);
        }
    }

    #[test]
    fn deterministic_with_same_seed() {
        let run = |seed: u64| -> Vec<(usize, u64)> {
            let config = SchedulerConfig {
                num_vcpus: 4,
                quantum: 10,
                strategy: SchedulingStrategy::Randomized {
                    min_quantum: 5,
                    max_quantum: 50,
                },
                seed,
            };
            let mut sched = VcpuScheduler::new(&config);
            let mut trace = Vec::new();

            for _ in 0..200 {
                if sched.tick() {
                    sched.advance();
                    trace.push((sched.active(), sched.remaining()));
                }
            }
            trace
        };

        let r1 = run(42);
        let r2 = run(42);
        let r3 = run(99);

        assert_eq!(r1, r2, "Same seed must produce same schedule");
        assert_ne!(r1, r3, "Different seeds should differ");
    }

    #[test]
    fn snapshot_restore_preserves_state() {
        let config = SchedulerConfig {
            num_vcpus: 3,
            quantum: 10,
            strategy: SchedulingStrategy::Randomized {
                min_quantum: 5,
                max_quantum: 30,
            },
            seed: 42,
        };
        let mut sched = VcpuScheduler::new(&config);

        // Advance some
        for _ in 0..50 {
            if sched.tick() {
                sched.advance();
            }
        }

        let snap = sched.snapshot();

        // Continue original
        let mut post_snap_trace = Vec::new();
        for _ in 0..100 {
            if sched.tick() {
                sched.advance();
                post_snap_trace.push((sched.active(), sched.remaining()));
            }
        }

        // Restore and replay
        let mut restored = VcpuScheduler::new(&config);
        restored.restore(&snap);
        let mut restored_trace = Vec::new();
        for _ in 0..100 {
            if restored.tick() {
                restored.advance();
                restored_trace.push((restored.active(), restored.remaining()));
            }
        }

        assert_eq!(post_snap_trace, restored_trace);
    }

    #[test]
    fn reset_returns_to_initial() {
        let config = SchedulerConfig {
            num_vcpus: 4,
            quantum: 20,
            ..Default::default()
        };
        let mut sched = VcpuScheduler::new(&config);

        // Advance
        for _ in 0..50 {
            if sched.tick() {
                sched.advance();
            }
        }
        assert_ne!(sched.active(), 0);

        sched.reset();
        assert_eq!(sched.active(), 0);
        assert_eq!(sched.remaining(), 20);
    }

    #[test]
    fn two_vcpus_alternates() {
        let config = SchedulerConfig {
            num_vcpus: 2,
            quantum: 3,
            strategy: SchedulingStrategy::RoundRobin,
            ..Default::default()
        };
        let mut sched = VcpuScheduler::new(&config);
        let mut trace = Vec::new();

        for _ in 0..18 {
            trace.push(sched.active());
            if sched.tick() {
                sched.advance();
            }
        }

        // 3 exits per vCPU: [0,0,0, 1,1,1, 0,0,0, 1,1,1, ...]
        assert_eq!(trace, vec![0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 1, 1]);
    }
}
