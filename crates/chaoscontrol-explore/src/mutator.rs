//! Fault schedule mutation — generates variant schedules from a base.

use chaoscontrol_fault::faults::Fault;
use chaoscontrol_fault::schedule::{FaultSchedule, ScheduledFault};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// Configuration for fault schedule mutation.
#[derive(Debug, Clone)]
pub struct MutationConfig {
    /// Number of VMs (for generating valid fault targets).
    pub num_vms: usize,
    /// Max simulation tick (for generating valid fault times).
    pub max_tick: u64,
    /// Probability of each mutation strategy [0.0, 1.0].
    pub add_prob: f64,
    pub remove_prob: f64,
    pub shift_prob: f64,
    pub replace_prob: f64,
}

impl Default for MutationConfig {
    fn default() -> Self {
        Self {
            num_vms: 2,
            max_tick: 1_000_000_000, // 1 second of virtual time
            add_prob: 0.4,
            remove_prob: 0.2,
            shift_prob: 0.2,
            replace_prob: 0.2,
        }
    }
}

/// Generates variant fault schedules from a base schedule.
pub struct ScheduleMutator {
    /// Master seed for deterministic mutation.
    seed: u64,
    /// Counter for generating unique child seeds.
    counter: u64,
}

impl ScheduleMutator {
    /// Create a new mutator with a master seed.
    pub fn new(seed: u64) -> Self {
        Self { seed, counter: 0 }
    }

    /// Generate N variant schedules from a base.
    ///
    /// Each variant is created by applying random mutations to the base schedule.
    /// Mutations are deterministic given the same seed and counter.
    pub fn mutate(
        &mut self,
        base: &FaultSchedule,
        n: usize,
        config: &MutationConfig,
    ) -> Vec<FaultSchedule> {
        let mut variants = Vec::with_capacity(n);

        for _ in 0..n {
            let variant = self.mutate_once(base, config);
            variants.push(variant);
        }

        variants
    }

    /// Generate a single variant schedule.
    fn mutate_once(&mut self, base: &FaultSchedule, config: &MutationConfig) -> FaultSchedule {
        let child_seed = self.seed.wrapping_add(self.counter);
        self.counter += 1;

        let mut rng = ChaCha8Rng::seed_from_u64(child_seed);

        // Clone the base schedule
        let mut schedule = base.clone();

        // Apply 1-3 random mutations
        let num_mutations = rng.gen_range(1..=3);
        for _ in 0..num_mutations {
            let roll = rng.gen::<f64>();
            let total_prob =
                config.add_prob + config.remove_prob + config.shift_prob + config.replace_prob;

            if total_prob == 0.0 {
                // No mutations configured
                break;
            }

            let norm_add = config.add_prob / total_prob;
            let norm_remove = norm_add + config.remove_prob / total_prob;
            let norm_shift = norm_remove + config.shift_prob / total_prob;

            if roll < norm_add {
                self.add_random_fault(&mut schedule, config, &mut rng);
            } else if roll < norm_remove {
                self.remove_random_fault(&mut schedule, &mut rng);
            } else if roll < norm_shift {
                self.shift_timing(&mut schedule, &mut rng);
            } else {
                self.replace_fault(&mut schedule, config, &mut rng);
            }
        }

        schedule
    }

    /// Mutation strategy 1: Add a random fault at a random time.
    fn add_random_fault(
        &mut self,
        schedule: &mut FaultSchedule,
        config: &MutationConfig,
        rng: &mut ChaCha8Rng,
    ) {
        let time_ns = rng.gen_range(0..=config.max_tick);
        let fault = self.random_fault(config, rng);
        schedule.add(ScheduledFault::new(time_ns, fault));
    }

    /// Mutation strategy 2: Remove a random fault.
    fn remove_random_fault(&mut self, schedule: &mut FaultSchedule, rng: &mut ChaCha8Rng) {
        if schedule.total() == 0 {
            return;
        }

        // Clone all faults, remove one randomly, rebuild schedule
        let faults: Vec<_> = {
            let mut sched_clone = schedule.clone();
            sched_clone.reset();
            let mut collected = Vec::new();
            while let Some(time) = sched_clone.next_time() {
                collected.extend(sched_clone.drain_due(time));
            }
            collected
        };

        if faults.is_empty() {
            return;
        }

        let remove_idx = rng.gen_range(0..faults.len());
        let mut new_schedule = FaultSchedule::new();
        for (i, fault) in faults.into_iter().enumerate() {
            if i != remove_idx {
                new_schedule.add(fault);
            }
        }

        *schedule = new_schedule;
    }

    /// Mutation strategy 3: Shift a fault's timing by ±10%.
    fn shift_timing(&mut self, schedule: &mut FaultSchedule, rng: &mut ChaCha8Rng) {
        let faults: Vec<_> = {
            let mut sched_clone = schedule.clone();
            sched_clone.reset();
            let mut collected = Vec::new();
            while let Some(time) = sched_clone.next_time() {
                collected.extend(sched_clone.drain_due(time));
            }
            collected
        };

        if faults.is_empty() {
            return;
        }

        let shift_idx = rng.gen_range(0..faults.len());
        let mut new_schedule = FaultSchedule::new();

        for (i, mut fault) in faults.into_iter().enumerate() {
            if i == shift_idx {
                let delta = (fault.time_ns / 10).max(1); // ±10%
                let shift = if rng.gen_bool(0.5) {
                    fault.time_ns.saturating_add(delta)
                } else {
                    fault.time_ns.saturating_sub(delta)
                };
                fault.time_ns = shift;
            }
            new_schedule.add(fault);
        }

        *schedule = new_schedule;
    }

    /// Mutation strategy 4: Replace a fault with a different type.
    fn replace_fault(
        &mut self,
        schedule: &mut FaultSchedule,
        config: &MutationConfig,
        rng: &mut ChaCha8Rng,
    ) {
        let faults: Vec<_> = {
            let mut sched_clone = schedule.clone();
            sched_clone.reset();
            let mut collected = Vec::new();
            while let Some(time) = sched_clone.next_time() {
                collected.extend(sched_clone.drain_due(time));
            }
            collected
        };

        if faults.is_empty() {
            return;
        }

        let replace_idx = rng.gen_range(0..faults.len());
        let mut new_schedule = FaultSchedule::new();

        for (i, mut fault) in faults.into_iter().enumerate() {
            if i == replace_idx {
                fault.fault = self.random_fault(config, rng);
            }
            new_schedule.add(fault);
        }

        *schedule = new_schedule;
    }

    /// Generate a random fault suitable for the config.
    fn random_fault(&self, config: &MutationConfig, rng: &mut ChaCha8Rng) -> Fault {
        if config.num_vms == 0 {
            return Fault::NetworkHeal;
        }

        let target = rng.gen_range(0..config.num_vms);
        let fault_type = rng.gen_range(0..13);

        match fault_type {
            0 => Fault::ProcessKill { target },
            1 => Fault::ProcessPause {
                target,
                duration_ns: rng.gen_range(1_000_000..10_000_000_000), // 1ms to 10s
            },
            2 => Fault::ProcessRestart { target },
            3 => {
                // Network partition: split randomly
                let side_a: Vec<usize> =
                    (0..config.num_vms).filter(|_| rng.gen_bool(0.5)).collect();
                let side_b: Vec<usize> = (0..config.num_vms)
                    .filter(|i| !side_a.contains(i))
                    .collect();

                if side_a.is_empty() || side_b.is_empty() {
                    Fault::NetworkHeal
                } else {
                    Fault::NetworkPartition { side_a, side_b }
                }
            }
            4 => Fault::NetworkHeal,
            5 => Fault::NetworkLatency {
                target,
                latency_ns: rng.gen_range(1_000_000..1_000_000_000), // 1ms to 1s
            },
            6 => Fault::DiskFull { target },
            7 => Fault::DiskWriteError {
                target,
                offset: rng.gen_range(0..10_000_000),
            },
            8 => Fault::ClockSkew {
                target,
                offset_ns: rng.gen_range(-10_000_000_000..10_000_000_000), // ±10s
            },
            9 => Fault::ClockJump {
                target,
                delta_ns: rng.gen_range(-5_000_000_000..5_000_000_000), // ±5s
            },
            10 => Fault::NetworkJitter {
                target,
                jitter_ns: rng.gen_range(1_000_000..100_000_000), // 1ms to 100ms
            },
            11 => Fault::NetworkBandwidth {
                target,
                bytes_per_sec: rng.gen_range(10_000..10_000_000), // 10 KB/s to 10 MB/s
            },
            12 => Fault::PacketDuplicate {
                target,
                rate_ppm: rng.gen_range(10_000..500_000), // 1% to 50%
            },
            _ => Fault::NetworkHeal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mutator_new() {
        let mutator = ScheduleMutator::new(42);
        assert_eq!(mutator.seed, 42);
        assert_eq!(mutator.counter, 0);
    }

    #[test]
    fn test_mutator_deterministic() {
        let base = FaultSchedule::new();
        let config = MutationConfig::default();

        let mut m1 = ScheduleMutator::new(100);
        let mut m2 = ScheduleMutator::new(100);

        let v1 = m1.mutate(&base, 5, &config);
        let v2 = m2.mutate(&base, 5, &config);

        // Same seed, same number of variants -> same total counts
        assert_eq!(v1.len(), v2.len());
        for (s1, s2) in v1.iter().zip(v2.iter()) {
            assert_eq!(s1.total(), s2.total());
        }
    }

    #[test]
    fn test_mutator_counter_increments() {
        let mut mutator = ScheduleMutator::new(42);
        let base = FaultSchedule::new();
        let config = MutationConfig::default();

        mutator.mutate(&base, 3, &config);
        assert_eq!(mutator.counter, 3);

        mutator.mutate(&base, 2, &config);
        assert_eq!(mutator.counter, 5);
    }

    #[test]
    fn test_add_random_fault() {
        let mut mutator = ScheduleMutator::new(42);
        let mut schedule = FaultSchedule::new();
        let config = MutationConfig::default();

        let mut rng = ChaCha8Rng::seed_from_u64(42);
        mutator.add_random_fault(&mut schedule, &config, &mut rng);

        assert!(schedule.total() > 0);
    }

    #[test]
    fn test_remove_random_fault() {
        let mut mutator = ScheduleMutator::new(42);
        let mut schedule = FaultSchedule::new();
        schedule.add(ScheduledFault::new(1000, Fault::NetworkHeal));
        schedule.add(ScheduledFault::new(2000, Fault::ProcessKill { target: 0 }));

        let mut rng = ChaCha8Rng::seed_from_u64(42);
        mutator.remove_random_fault(&mut schedule, &mut rng);

        assert_eq!(schedule.total(), 1);
    }

    #[test]
    fn test_remove_from_empty() {
        let mut mutator = ScheduleMutator::new(42);
        let mut schedule = FaultSchedule::new();
        let mut rng = ChaCha8Rng::seed_from_u64(42);

        mutator.remove_random_fault(&mut schedule, &mut rng);
        assert_eq!(schedule.total(), 0); // No crash
    }

    #[test]
    fn test_shift_timing() {
        let mut mutator = ScheduleMutator::new(42);
        let mut schedule = FaultSchedule::new();
        schedule.add(ScheduledFault::new(10000, Fault::NetworkHeal));

        let mut rng = ChaCha8Rng::seed_from_u64(42);
        mutator.shift_timing(&mut schedule, &mut rng);

        // Time should have shifted
        let new_time = schedule.next_time().unwrap();
        assert_ne!(new_time, 10000);
    }

    #[test]
    fn test_replace_fault() {
        let mut mutator = ScheduleMutator::new(42);
        let mut schedule = FaultSchedule::new();
        schedule.add(ScheduledFault::new(1000, Fault::NetworkHeal));

        let config = MutationConfig {
            num_vms: 3,
            ..Default::default()
        };

        let mut rng = ChaCha8Rng::seed_from_u64(42);
        mutator.replace_fault(&mut schedule, &config, &mut rng);

        assert_eq!(schedule.total(), 1);
        // Fault type might have changed (can't assert exact type due to randomness)
    }

    #[test]
    fn test_random_fault_variety() {
        let mutator = ScheduleMutator::new(42);
        let config = MutationConfig {
            num_vms: 3,
            ..Default::default()
        };

        let mut rng = ChaCha8Rng::seed_from_u64(999);
        let mut fault_debug_strs = std::collections::BTreeSet::new();

        for _ in 0..100 {
            let fault = mutator.random_fault(&config, &mut rng);
            // Use debug string representation to track variety
            let debug_str = format!("{:?}", std::mem::discriminant(&fault));
            fault_debug_strs.insert(debug_str);
        }

        // Should generate multiple fault types
        assert!(fault_debug_strs.len() > 3);
    }

    #[test]
    fn test_mutate_applies_multiple_mutations() {
        let mut mutator = ScheduleMutator::new(42);
        let base = FaultSchedule::new();
        let config = MutationConfig::default();

        let variants = mutator.mutate(&base, 10, &config);
        assert_eq!(variants.len(), 10);

        // Each variant should have some faults (due to add mutations)
        let total_faults: usize = variants.iter().map(|v| v.total()).sum();
        assert!(total_faults > 0);
    }

    #[test]
    fn test_zero_vms_config() {
        let mutator = ScheduleMutator::new(42);
        let config = MutationConfig {
            num_vms: 0,
            ..Default::default()
        };

        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let fault = mutator.random_fault(&config, &mut rng);
        assert_eq!(fault, Fault::NetworkHeal); // Safe default
    }
}
