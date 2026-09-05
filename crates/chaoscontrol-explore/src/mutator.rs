//! Fault schedule mutation — generates variant schedules from a base.

use rand::{Rng, SeedableRng};

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
    /// Fraction of mutations that target scheduling (0.0 = disabled).
    /// When > 0, each `mutate_with_schedule` call has this probability
    /// of generating a schedule variant instead of mutating faults.
    pub schedule_mutation_ratio: f64,
    /// Base quantum for QuantumShift mutations.
    pub base_quantum: u64,
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
            schedule_mutation_ratio: 0.0,
            base_quantum: 100,
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
        base: &::chaoscontrol_fault::schedule::FaultSchedule,
        n: usize,
        config: &MutationConfig,
    ) -> Vec<::chaoscontrol_fault::schedule::FaultSchedule> {
        let mut variants = Vec::with_capacity(n);

        for _ in 0..n {
            let variant = self.mutate_once(base, config);
            variants.push(variant);
        }

        variants
    }

    /// Generate N variant schedules using havoc mode (aggressive mutations).
    ///
    /// Each variant applies `mutations_range[0]..=mutations_range[1]`
    /// random mutations instead of the normal 1–3.
    pub fn mutate_havoc(
        &mut self,
        base: &::chaoscontrol_fault::schedule::FaultSchedule,
        n: usize,
        config: &MutationConfig,
        mutations_range: [u32; 2],
    ) -> Vec<::chaoscontrol_fault::schedule::FaultSchedule> {
        let mut variants = Vec::with_capacity(n);
        let lo = mutations_range[0].max(1);
        let hi = mutations_range[1].max(lo);

        for _ in 0..n {
            let child_seed = self.seed.wrapping_add(self.counter);
            self.counter += 1;
            let mut rng = ::rand_chacha::ChaCha8Rng::seed_from_u64(child_seed);

            let mut schedule = base.clone();

            let num_mutations = rng.gen_range(lo..=hi);
            for _ in 0..num_mutations {
                let strategy = rng.gen_range(0u8..5);
                match strategy {
                    0 | 1 => self.add_random_fault(&mut schedule, config, &mut rng),
                    2 => self.remove_random_fault(&mut schedule, &mut rng),
                    3 => self.shift_timing(&mut schedule, &mut rng),
                    _ => self.replace_fault(&mut schedule, config, &mut rng),
                }
            }

            variants.push(schedule);
        }

        variants
    }

    /// Generate aggressive fault schedules and optional scheduler variants.
    pub fn mutate_havoc_with_schedule(
        &mut self,
        base: &::chaoscontrol_fault::schedule::FaultSchedule,
        n: usize,
        config: &MutationConfig,
        mutations_range: [u32; 2],
    ) -> Vec<(
        ::chaoscontrol_fault::schedule::FaultSchedule,
        Option<::chaoscontrol_vmm::scheduler::ScheduleVariant>,
    )> {
        let schedules = self.mutate_havoc(base, n, config, mutations_range);
        schedules
            .into_iter()
            .map(|schedule| {
                let child_seed = self.seed.wrapping_add(self.counter);
                self.counter = self.counter.wrapping_add(1);
                let mut rng = ::rand_chacha::ChaCha8Rng::seed_from_u64(child_seed);
                let variant = self.schedule_variant(config, &mut rng);
                (schedule, variant)
            })
            .collect()
    }

    /// Generate a single variant schedule.
    fn mutate_once(
        &mut self,
        base: &::chaoscontrol_fault::schedule::FaultSchedule,
        config: &MutationConfig,
    ) -> ::chaoscontrol_fault::schedule::FaultSchedule {
        let child_seed = self.seed.wrapping_add(self.counter);
        self.counter += 1;

        let mut rng = ::rand_chacha::ChaCha8Rng::seed_from_u64(child_seed);

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
        schedule: &mut ::chaoscontrol_fault::schedule::FaultSchedule,
        config: &MutationConfig,
        rng: &mut ::rand_chacha::ChaCha8Rng,
    ) {
        let time_ns = rng.gen_range(0..=config.max_tick);
        let fault = self.random_fault(config, rng);
        schedule.add(::chaoscontrol_fault::schedule::ScheduledFault::new(
            time_ns, fault,
        ));
    }

    /// Mutation strategy 2: Remove a random fault.
    fn remove_random_fault(
        &mut self,
        schedule: &mut ::chaoscontrol_fault::schedule::FaultSchedule,
        rng: &mut ::rand_chacha::ChaCha8Rng,
    ) {
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
        let mut new_schedule = ::chaoscontrol_fault::schedule::FaultSchedule::new();
        for (i, fault) in faults.into_iter().enumerate() {
            if i != remove_idx {
                new_schedule.add(fault);
            }
        }

        *schedule = new_schedule;
    }

    /// Mutation strategy 3: Shift a fault's timing by ±10%.
    fn shift_timing(
        &mut self,
        schedule: &mut ::chaoscontrol_fault::schedule::FaultSchedule,
        rng: &mut ::rand_chacha::ChaCha8Rng,
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

        let shift_idx = rng.gen_range(0..faults.len());
        let mut new_schedule = ::chaoscontrol_fault::schedule::FaultSchedule::new();

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
        schedule: &mut ::chaoscontrol_fault::schedule::FaultSchedule,
        config: &MutationConfig,
        rng: &mut ::rand_chacha::ChaCha8Rng,
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
        let mut new_schedule = ::chaoscontrol_fault::schedule::FaultSchedule::new();

        for (i, mut fault) in faults.into_iter().enumerate() {
            if i == replace_idx {
                fault.fault = self.random_fault(config, rng);
            }
            new_schedule.add(fault);
        }

        *schedule = new_schedule;
    }

    /// Generate N (fault schedule, schedule variant) pairs.
    ///
    /// Each pair may have a schedule variant if schedule diversity is
    /// enabled (schedule_mutation_ratio > 0). The variant is `None`
    /// when the mutation targets faults only.
    pub fn mutate_with_schedule(
        &mut self,
        base: &::chaoscontrol_fault::schedule::FaultSchedule,
        n: usize,
        config: &MutationConfig,
    ) -> Vec<(
        ::chaoscontrol_fault::schedule::FaultSchedule,
        Option<::chaoscontrol_vmm::scheduler::ScheduleVariant>,
    )> {
        let mut results = Vec::with_capacity(n);
        for _ in 0..n {
            let child_seed = self.seed.wrapping_add(self.counter);
            self.counter += 1;
            let mut rng = ::rand_chacha::ChaCha8Rng::seed_from_u64(child_seed);

            let variant = self.schedule_variant(config, &mut rng);

            // Always mutate the fault schedule too
            let schedule = self.mutate_once(base, config);
            results.push((schedule, variant));
        }
        results
    }

    fn schedule_variant(
        &self,
        config: &MutationConfig,
        rng: &mut ::rand_chacha::ChaCha8Rng,
    ) -> Option<::chaoscontrol_vmm::scheduler::ScheduleVariant> {
        if config.schedule_mutation_ratio > 0.0 && rng.gen::<f64>() < config.schedule_mutation_ratio
        {
            Some(self.random_schedule_variant(config, rng))
        } else {
            None
        }
    }

    /// Generate a random `ScheduleVariant`.
    ///
    /// Picks one of three operators: ReSeed (50%), QuantumShift (30%),
    /// StrategyFlip (20%).
    fn random_schedule_variant(
        &self,
        config: &MutationConfig,
        rng: &mut ::rand_chacha::ChaCha8Rng,
    ) -> ::chaoscontrol_vmm::scheduler::ScheduleVariant {
        let scheduler_seed = rng.gen::<u64>();
        let roll = rng.gen::<f64>();

        if roll < 0.5 {
            // ReSeed only — different interleaving, same strategy/quantum
            ::chaoscontrol_vmm::scheduler::ScheduleVariant {
                scheduler_seed,
                ..Default::default()
            }
        } else if roll < 0.8 {
            // QuantumShift — multiply or divide by 2-8×
            let factor = rng.gen_range(2u64..=8);
            let quantum = if rng.gen_bool(0.5) {
                config.base_quantum.saturating_mul(factor)
            } else {
                (config.base_quantum / factor).max(1)
            };
            ::chaoscontrol_vmm::scheduler::ScheduleVariant {
                scheduler_seed,
                quantum_override: Some(quantum),
                ..Default::default()
            }
        } else {
            // StrategyFlip — toggle between RoundRobin and Randomized
            let strategy = if rng.gen_bool(0.5) {
                ::chaoscontrol_vmm::scheduler::SchedulingStrategy::RoundRobin
            } else {
                let min_q = (config.base_quantum / 4).max(1);
                let max_q = config.base_quantum.saturating_mul(4);
                ::chaoscontrol_vmm::scheduler::SchedulingStrategy::Randomized {
                    min_quantum: min_q,
                    max_quantum: max_q,
                }
            };
            ::chaoscontrol_vmm::scheduler::ScheduleVariant {
                scheduler_seed,
                strategy_override: Some(strategy),
                ..Default::default()
            }
        }
    }

    /// Generate a random fault suitable for the config.
    fn random_fault(
        &self,
        config: &MutationConfig,
        rng: &mut ::rand_chacha::ChaCha8Rng,
    ) -> ::chaoscontrol_fault::faults::Fault {
        if config.num_vms == 0 {
            return ::chaoscontrol_fault::faults::Fault::NetworkHeal;
        }

        let target = rng.gen_range(0..config.num_vms);
        let fault_type = rng.gen_range(0..22);

        match fault_type {
            0 => ::chaoscontrol_fault::faults::Fault::ProcessKill { target },
            1 => ::chaoscontrol_fault::faults::Fault::ProcessPause {
                target,
                duration_ns: rng.gen_range(1_000_000..10_000_000_000), // 1ms to 10s
            },
            2 => ::chaoscontrol_fault::faults::Fault::ProcessRestart { target },
            3 => {
                // Network partition: split randomly
                let side_a: Vec<usize> =
                    (0..config.num_vms).filter(|_| rng.gen_bool(0.5)).collect();
                let side_b: Vec<usize> = (0..config.num_vms)
                    .filter(|i| !side_a.contains(i))
                    .collect();

                if side_a.is_empty() || side_b.is_empty() {
                    ::chaoscontrol_fault::faults::Fault::NetworkHeal
                } else {
                    ::chaoscontrol_fault::faults::Fault::NetworkPartition { side_a, side_b }
                }
            }
            4 => ::chaoscontrol_fault::faults::Fault::NetworkHeal,
            5 => ::chaoscontrol_fault::faults::Fault::NetworkLatency {
                target,
                latency_ns: rng.gen_range(1_000_000..1_000_000_000), // 1ms to 1s
            },
            6 => ::chaoscontrol_fault::faults::Fault::DiskFull { target },
            7 => ::chaoscontrol_fault::faults::Fault::DiskWriteError {
                target,
                offset: rng.gen_range(0..10_000_000),
            },
            8 => ::chaoscontrol_fault::faults::Fault::ClockSkew {
                target,
                offset_ns: rng.gen_range(-10_000_000_000..10_000_000_000), // ±10s
            },
            9 => ::chaoscontrol_fault::faults::Fault::ClockJump {
                target,
                delta_ns: rng.gen_range(-5_000_000_000..5_000_000_000), // ±5s
            },
            10 => ::chaoscontrol_fault::faults::Fault::NetworkJitter {
                target,
                jitter_ns: rng.gen_range(1_000_000..100_000_000), // 1ms to 100ms
            },
            11 => ::chaoscontrol_fault::faults::Fault::NetworkBandwidth {
                target,
                bytes_per_sec: rng.gen_range(10_000..10_000_000), // 10 KB/s to 10 MB/s
            },
            12 => ::chaoscontrol_fault::faults::Fault::PacketDuplicate {
                target,
                rate_ppm: rng.gen_range(10_000..500_000), // 1% to 50%
            },
            13 => ::chaoscontrol_fault::faults::Fault::InjectInterrupt {
                target,
                irq: rng.gen_range(0..24), // Full x86 PIC/IOAPIC range
            },
            14 => ::chaoscontrol_fault::faults::Fault::InjectNmi { target, vcpu: 0 },
            15 => ::chaoscontrol_fault::faults::Fault::DiskSlow {
                target,
                delay_ns: rng.gen_range(1_000_000..100_000_000), // 1ms to 100ms
            },
            16 => ::chaoscontrol_fault::faults::Fault::DiskFsyncLie { target },
            17 => ::chaoscontrol_fault::faults::Fault::DiskPartialRead {
                target,
                offset: rng.gen_range(0..10_000_000),
                max_bytes: rng.gen_range(1..4096),
            },
            18 => ::chaoscontrol_fault::faults::Fault::CpuBitflip {
                target,
                vcpu: 0,
                register: ::chaoscontrol_fault::faults::GpRegister::ALL[rng.gen_range(0..16)],
                bit: rng.gen_range(0..64),
            },
            19 => ::chaoscontrol_fault::faults::Fault::CpuStall {
                target,
                vcpu: 0,
                duration_ticks: rng.gen_range(1..200),
            },
            20 => ::chaoscontrol_fault::faults::Fault::ClockFreeze {
                target,
                duration_ticks: rng.gen_range(10..500),
            },
            21 => ::chaoscontrol_fault::faults::Fault::ClockJitter {
                target,
                bound_tsc: rng.gen_range(100..5000),
            },
            _ => ::chaoscontrol_fault::faults::Fault::NetworkHeal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chaoscontrol_vmm::scheduler::core::{
        ProgressMode, ProgressSource, ScheduleAction, ScheduleEvent,
    };
    use chaoscontrol_vmm::scheduler::{SchedulerConfig, VcpuScheduler};

    const RACE_VCPU_COUNT: usize = 2;
    const RACE_QUANTUM: u64 = 100;
    const RACE_SEED: u64 = 41;
    const RACE_VARIANT_COUNT: usize = 128;
    const RACE_TRIGGER_MAXIMUM_STEPS: usize = 25;
    const RACE_MAXIMUM_STEPS: usize = 512;
    const HAVOC_MUTATION_RANGE: [u32; 2] = [4, 16];
    const INVALID_MINIMUM_QUANTUM: u64 = 9;
    const INVALID_MAXIMUM_QUANTUM: u64 = 2;

    fn first_switch_steps(
        variant: Option<&::chaoscontrol_vmm::scheduler::ScheduleVariant>,
    ) -> Option<usize> {
        let config = SchedulerConfig {
            num_vcpus: RACE_VCPU_COUNT,
            quantum: RACE_QUANTUM,
            strategy: ::chaoscontrol_vmm::scheduler::SchedulingStrategy::RoundRobin,
            seed: RACE_SEED,
        };
        let mut scheduler = VcpuScheduler::try_new(
            &config,
            ProgressMode::ExactSingleStep,
            vec![true; RACE_VCPU_COUNT],
        )
        .expect("race scheduler");
        if let Some(variant) = variant {
            scheduler.apply_variant(variant).expect("race variant");
        }
        for step in 1..=RACE_MAXIMUM_STEPS {
            let state = scheduler.state();
            let event = ScheduleEvent::GuestProgress {
                expected_state_id: state.identity(),
                vcpu: state.active_vcpu,
                observed_progress: state.instruction_progress[state.active_vcpu].saturating_add(1),
                runnable_changes: Vec::new(),
                source: ProgressSource::ExactSingleStep,
            };
            let reservation = scheduler.reserve_transition().expect("race reservation");
            let planned = scheduler.plan(&event).expect("race plan");
            let action = planned.record.action.clone();
            scheduler
                .commit(reservation, planned)
                .expect("race transition");
            if matches!(action, ScheduleAction::Switch { .. }) {
                return Some(step);
            }
        }
        None
    }

    #[test]
    fn test_mutator_new() {
        let mutator = ScheduleMutator::new(42);
        assert_eq!(mutator.seed, 42);
        assert_eq!(mutator.counter, 0);
    }

    #[test]
    fn test_mutator_deterministic() {
        let base = ::chaoscontrol_fault::schedule::FaultSchedule::new();
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
        let base = ::chaoscontrol_fault::schedule::FaultSchedule::new();
        let config = MutationConfig::default();

        mutator.mutate(&base, 3, &config);
        assert_eq!(mutator.counter, 3);

        mutator.mutate(&base, 2, &config);
        assert_eq!(mutator.counter, 5);
    }

    #[test]
    fn test_add_random_fault() {
        let mut mutator = ScheduleMutator::new(42);
        let mut schedule = ::chaoscontrol_fault::schedule::FaultSchedule::new();
        let config = MutationConfig::default();

        let mut rng = ::rand_chacha::ChaCha8Rng::seed_from_u64(42);
        mutator.add_random_fault(&mut schedule, &config, &mut rng);

        assert!(schedule.total() > 0);
    }

    #[test]
    fn test_remove_random_fault() {
        let mut mutator = ScheduleMutator::new(42);
        let mut schedule = ::chaoscontrol_fault::schedule::FaultSchedule::new();
        schedule.add(::chaoscontrol_fault::schedule::ScheduledFault::new(
            1000,
            ::chaoscontrol_fault::faults::Fault::NetworkHeal,
        ));
        schedule.add(::chaoscontrol_fault::schedule::ScheduledFault::new(
            2000,
            ::chaoscontrol_fault::faults::Fault::ProcessKill { target: 0 },
        ));

        let mut rng = ::rand_chacha::ChaCha8Rng::seed_from_u64(42);
        mutator.remove_random_fault(&mut schedule, &mut rng);

        assert_eq!(schedule.total(), 1);
    }

    #[test]
    fn test_remove_from_empty() {
        let mut mutator = ScheduleMutator::new(42);
        let mut schedule = ::chaoscontrol_fault::schedule::FaultSchedule::new();
        let mut rng = ::rand_chacha::ChaCha8Rng::seed_from_u64(42);

        mutator.remove_random_fault(&mut schedule, &mut rng);
        assert_eq!(schedule.total(), 0); // No crash
    }

    #[test]
    fn test_shift_timing() {
        let mut mutator = ScheduleMutator::new(42);
        let mut schedule = ::chaoscontrol_fault::schedule::FaultSchedule::new();
        schedule.add(::chaoscontrol_fault::schedule::ScheduledFault::new(
            10000,
            ::chaoscontrol_fault::faults::Fault::NetworkHeal,
        ));

        let mut rng = ::rand_chacha::ChaCha8Rng::seed_from_u64(42);
        mutator.shift_timing(&mut schedule, &mut rng);

        // Time should have shifted
        let new_time = schedule.next_time().unwrap();
        assert_ne!(new_time, 10000);
    }

    #[test]
    fn test_replace_fault() {
        let mut mutator = ScheduleMutator::new(42);
        let mut schedule = ::chaoscontrol_fault::schedule::FaultSchedule::new();
        schedule.add(::chaoscontrol_fault::schedule::ScheduledFault::new(
            1000,
            ::chaoscontrol_fault::faults::Fault::NetworkHeal,
        ));

        let config = MutationConfig {
            num_vms: 3,
            ..Default::default()
        };

        let mut rng = ::rand_chacha::ChaCha8Rng::seed_from_u64(42);
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

        let mut rng = ::rand_chacha::ChaCha8Rng::seed_from_u64(999);
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
        let base = ::chaoscontrol_fault::schedule::FaultSchedule::new();
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

        let mut rng = ::rand_chacha::ChaCha8Rng::seed_from_u64(42);
        let fault = mutator.random_fault(&config, &mut rng);
        assert_eq!(fault, ::chaoscontrol_fault::faults::Fault::NetworkHeal); // Safe default
    }

    #[test]
    fn test_havoc_generates_more_faults() {
        let mut mutator_normal = ScheduleMutator::new(42);
        let mut mutator_havoc = ScheduleMutator::new(42);
        let config = MutationConfig::default();
        let base = ::chaoscontrol_fault::schedule::FaultSchedule::new();

        let normal = mutator_normal.mutate(&base, 10, &config);
        let havoc = mutator_havoc.mutate_havoc(&base, 10, &config, [4, 16]);

        assert_eq!(normal.len(), 10);
        assert_eq!(havoc.len(), 10);

        // Havoc should produce more total faults on average
        let normal_faults: usize = normal.iter().map(|s| s.total()).sum();
        let havoc_faults: usize = havoc.iter().map(|s| s.total()).sum();
        assert!(
            havoc_faults > normal_faults,
            "havoc ({}) should produce more faults than normal ({})",
            havoc_faults,
            normal_faults
        );
    }

    #[test]
    // r[verify chaoscontrol.schedule_diversity.validated_effectiveness]
    fn schedule_variants_reach_known_ordering_race() {
        let base = ::chaoscontrol_fault::schedule::FaultSchedule::new();
        let config = MutationConfig {
            schedule_mutation_ratio: 1.0,
            base_quantum: RACE_QUANTUM,
            ..Default::default()
        };
        let mut mutator = ScheduleMutator::new(RACE_SEED);
        let variants = mutator.mutate_with_schedule(&base, RACE_VARIANT_COUNT, &config);

        assert!(first_switch_steps(None).is_some_and(|steps| steps > RACE_TRIGGER_MAXIMUM_STEPS));
        assert!(variants.iter().any(|(_schedule, variant)| {
            variant.as_ref().is_some_and(|variant| {
                first_switch_steps(Some(variant))
                    .is_some_and(|steps| steps <= RACE_TRIGGER_MAXIMUM_STEPS)
            })
        }));
    }

    #[test]
    // r[verify chaoscontrol.schedule_diversity.validation]
    fn disabled_and_invalid_schedule_variants_fail_closed() {
        let base = ::chaoscontrol_fault::schedule::FaultSchedule::new();
        let mut disabled = ScheduleMutator::new(RACE_SEED);
        let variants =
            disabled.mutate_with_schedule(&base, RACE_VARIANT_COUNT, &MutationConfig::default());
        assert!(variants
            .iter()
            .all(|(_schedule, variant)| variant.is_none()));

        let config = SchedulerConfig {
            num_vcpus: RACE_VCPU_COUNT,
            quantum: RACE_QUANTUM,
            strategy: ::chaoscontrol_vmm::scheduler::SchedulingStrategy::RoundRobin,
            seed: RACE_SEED,
        };
        let mut scheduler = VcpuScheduler::try_new(
            &config,
            ProgressMode::ExactSingleStep,
            vec![true; RACE_VCPU_COUNT],
        )
        .expect("validation scheduler");
        let unsupported = ::chaoscontrol_vmm::scheduler::ScheduleVariant {
            scheduler_seed: RACE_SEED,
            strategy_override: Some(
                ::chaoscontrol_vmm::scheduler::SchedulingStrategy::Randomized {
                    min_quantum: INVALID_MINIMUM_QUANTUM,
                    max_quantum: INVALID_MAXIMUM_QUANTUM,
                },
            ),
            quantum_override: None,
        };
        assert!(scheduler.apply_variant(&unsupported).is_err());
        let zero_quantum = ::chaoscontrol_vmm::scheduler::ScheduleVariant {
            scheduler_seed: RACE_SEED,
            strategy_override: None,
            quantum_override: Some(0),
        };
        assert!(scheduler.apply_variant(&zero_quantum).is_err());
    }

    #[test]
    fn havoc_schedule_variants_are_deterministic() {
        let base = ::chaoscontrol_fault::schedule::FaultSchedule::new();
        let config = MutationConfig {
            schedule_mutation_ratio: 1.0,
            ..Default::default()
        };
        let mut first = ScheduleMutator::new(RACE_SEED);
        let mut second = ScheduleMutator::new(RACE_SEED);
        let first_variants = first.mutate_havoc_with_schedule(
            &base,
            RACE_VARIANT_COUNT,
            &config,
            HAVOC_MUTATION_RANGE,
        );
        let second_variants = second.mutate_havoc_with_schedule(
            &base,
            RACE_VARIANT_COUNT,
            &config,
            HAVOC_MUTATION_RANGE,
        );
        assert_eq!(first_variants.len(), second_variants.len());
        for (first, second) in first_variants.iter().zip(second_variants.iter()) {
            assert_eq!(first.0.total(), second.0.total());
            assert_eq!(first.1, second.1);
        }
        assert!(first_variants
            .iter()
            .all(|(_schedule, variant)| variant.is_some()));
    }

    #[test]
    fn test_havoc_deterministic() {
        let config = MutationConfig::default();
        let base = ::chaoscontrol_fault::schedule::FaultSchedule::new();

        let mut m1 = ScheduleMutator::new(99);
        let mut m2 = ScheduleMutator::new(99);

        let r1 = m1.mutate_havoc(&base, 5, &config, [4, 16]);
        let r2 = m2.mutate_havoc(&base, 5, &config, [4, 16]);

        for (a, b) in r1.iter().zip(r2.iter()) {
            assert_eq!(a.total(), b.total());
        }
    }
}
