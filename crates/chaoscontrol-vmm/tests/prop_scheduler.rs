//! Deterministic sweep tests for the pure-authority vCPU scheduler.

use chaoscontrol_vmm::scheduler::core::{
    ProgressMode, ProgressSource, ScheduleAction, ScheduleError, ScheduleEvent,
};
use chaoscontrol_vmm::scheduler::{SchedulerConfig, SchedulingStrategy, VcpuScheduler};

const WIDE_CASES: u64 = 300;
const NORMAL_CASES: u64 = 200;
const MIN_VCPUS: usize = 1;
const MAX_VCPUS: usize = 16;
const MIN_QUANTUM: u64 = 1;
const MAX_QUANTUM: u64 = 500;
const MIN_TICKS: usize = 10;
const MAX_TICKS: usize = 2_000;

#[derive(Clone)]
struct DeterministicCase {
    state: u64,
}

impl DeterministicCase {
    fn new(index: u64) -> Self {
        const CASE_DOMAIN: u64 = 0xa24b_aed4_963e_e407;
        Self {
            state: index ^ CASE_DOMAIN,
        }
    }

    fn next(&mut self) -> u64 {
        const MULTIPLIER: u64 = 6_364_136_223_846_793_005;
        const INCREMENT: u64 = 1_442_695_040_888_963_407;
        self.state = self.state.wrapping_mul(MULTIPLIER).wrapping_add(INCREMENT);
        self.state
    }

    fn u64(&mut self, min: u64, max: u64) -> u64 {
        min + (self.next() % (max - min + 1))
    }

    fn usize(&mut self, min: usize, max: usize) -> usize {
        min + (self.next() as usize % (max - min + 1))
    }
}

fn scheduler(config: &SchedulerConfig) -> VcpuScheduler {
    VcpuScheduler::try_new(
        config,
        ProgressMode::ExactSingleStep,
        vec![true; config.num_vcpus],
    )
    .unwrap()
}

fn step(scheduler: &mut VcpuScheduler) -> ScheduleAction {
    let state = scheduler.state();
    let event = ScheduleEvent::GuestProgress {
        expected_state_id: state.identity(),
        vcpu: state.active_vcpu,
        observed_progress: state.instruction_progress[state.active_vcpu] + 1,
        runnable_changes: Vec::new(),
        source: ProgressSource::ExactSingleStep,
    };
    let reservation = scheduler.reserve_transition().unwrap();
    let planned = scheduler.plan(&event).unwrap();
    let action = planned.record.action.clone();
    scheduler.commit(reservation, planned).unwrap();
    action
}

fn run_trace(scheduler: &mut VcpuScheduler, ticks: usize) -> Vec<(usize, u64)> {
    let mut trace = Vec::new();
    for _ in 0..ticks {
        if matches!(step(scheduler), ScheduleAction::Switch { .. }) {
            trace.push((scheduler.active(), scheduler.remaining()));
        }
    }
    trace
}

#[test]
fn deterministic_same_seed_same_trace() {
    for case in 0..WIDE_CASES {
        let mut test_case = DeterministicCase::new(case);
        let num_vcpus = test_case.usize(MIN_VCPUS, MAX_VCPUS);
        let quantum = test_case.u64(MIN_QUANTUM, MAX_QUANTUM);
        let seed = test_case.next();
        let ticks = test_case.usize(MIN_TICKS, MAX_TICKS);
        let min_quantum = test_case.u64(MIN_QUANTUM, MAX_QUANTUM / 10);
        let max_quantum = test_case.u64(min_quantum + 1, MAX_QUANTUM);
        let config = SchedulerConfig {
            num_vcpus,
            quantum,
            strategy: SchedulingStrategy::Randomized {
                min_quantum,
                max_quantum,
            },
            seed,
        };
        let mut first = scheduler(&config);
        let mut second = scheduler(&config);
        assert_eq!(run_trace(&mut first, ticks), run_trace(&mut second, ticks));
        assert_eq!(first.state_id(), second.state_id(), "case {case}");
    }
}

#[test]
fn snapshot_restore_produces_same_future() {
    for case in 0..WIDE_CASES {
        let mut test_case = DeterministicCase::new(case);
        let num_vcpus = test_case.usize(2, MAX_VCPUS / 2);
        let quantum = test_case.u64(MIN_QUANTUM, MAX_QUANTUM);
        let pre_ticks = test_case.usize(MIN_QUANTUM as usize, MAX_QUANTUM as usize);
        let post_ticks = test_case.usize(MIN_TICKS, MAX_QUANTUM as usize);
        let config = SchedulerConfig {
            num_vcpus,
            quantum,
            strategy: SchedulingStrategy::RoundRobin,
            seed: test_case.next(),
        };
        let mut original = scheduler(&config);
        run_trace(&mut original, pre_ticks);
        let snapshot = original.snapshot();
        let mut restored = scheduler(&config);
        restored.restore(&snapshot).unwrap();
        assert_eq!(
            run_trace(&mut original, post_ticks),
            run_trace(&mut restored, post_ticks),
            "case {case}"
        );
    }
}

#[test]
fn active_vcpu_is_always_in_bounds() {
    for case in 0..WIDE_CASES {
        let mut test_case = DeterministicCase::new(case);
        let num_vcpus = test_case.usize(MIN_VCPUS, MAX_VCPUS);
        let config = SchedulerConfig {
            num_vcpus,
            quantum: test_case.u64(MIN_QUANTUM, MAX_QUANTUM),
            strategy: SchedulingStrategy::RoundRobin,
            seed: test_case.next(),
        };
        let mut scheduler = scheduler(&config);
        let ticks = test_case.usize(MIN_TICKS, MAX_TICKS);
        for _ in 0..ticks {
            assert!(scheduler.active() < num_vcpus, "case {case}");
            step(&mut scheduler);
        }
    }
}

#[test]
fn single_vcpu_never_selects_another_vcpu() {
    for case in 0..NORMAL_CASES {
        let mut test_case = DeterministicCase::new(case);
        let config = SchedulerConfig {
            num_vcpus: 1,
            quantum: test_case.u64(MIN_QUANTUM, MAX_QUANTUM),
            strategy: SchedulingStrategy::RoundRobin,
            seed: test_case.next(),
        };
        let mut scheduler = scheduler(&config);
        for _ in 0..test_case.usize(MIN_TICKS, MAX_TICKS) {
            step(&mut scheduler);
            assert_eq!(scheduler.active(), 0, "case {case}");
        }
    }
}

#[test]
fn randomized_quantum_stays_in_declared_range() {
    for case in 0..NORMAL_CASES {
        let mut test_case = DeterministicCase::new(case);
        let min_quantum = test_case.u64(MIN_QUANTUM, MAX_QUANTUM / 10);
        let max_quantum = test_case.u64(min_quantum + 1, MAX_QUANTUM);
        let config = SchedulerConfig {
            num_vcpus: test_case.usize(2, MAX_VCPUS / 2),
            quantum: MIN_QUANTUM,
            strategy: SchedulingStrategy::Randomized {
                min_quantum,
                max_quantum,
            },
            seed: test_case.next(),
        };
        let mut scheduler = scheduler(&config);
        for (_, quantum) in run_trace(&mut scheduler, MAX_TICKS) {
            assert!(
                (min_quantum..max_quantum).contains(&quantum),
                "case {case}: quantum {quantum}"
            );
        }
    }
}

#[test]
fn stale_event_rejection_preserves_state() {
    let config = SchedulerConfig {
        num_vcpus: 2,
        quantum: MAX_QUANTUM,
        strategy: SchedulingStrategy::RoundRobin,
        seed: 0,
    };
    let mut scheduler = scheduler(&config);
    let before = scheduler.state().clone();
    let event = ScheduleEvent::GuestProgress {
        expected_state_id: chaoscontrol_vmm::scheduler::core::ScheduleStateId([0; 32]),
        vcpu: 0,
        observed_progress: 1,
        runnable_changes: Vec::new(),
        source: ProgressSource::ExactSingleStep,
    };
    let reservation = scheduler.reserve_transition().unwrap();
    assert!(matches!(
        scheduler.plan(&event),
        Err(ScheduleError::StaleState { .. })
    ));
    scheduler.release_transition(reservation).unwrap();
    assert_eq!(scheduler.state(), &before);
}

#[test]
fn round_robin_visits_all_vcpus() {
    for case in 0..NORMAL_CASES {
        let mut test_case = DeterministicCase::new(case);
        let num_vcpus = test_case.usize(2, MAX_VCPUS / 2);
        let quantum = test_case.u64(MIN_QUANTUM, MAX_QUANTUM / 25);
        let config = SchedulerConfig {
            num_vcpus,
            quantum,
            strategy: SchedulingStrategy::RoundRobin,
            seed: test_case.next(),
        };
        let mut scheduler = scheduler(&config);
        let mut seen = vec![false; num_vcpus];
        let total_ticks = num_vcpus * quantum as usize * 2;
        for _ in 0..total_ticks {
            seen[scheduler.active()] = true;
            step(&mut scheduler);
        }
        assert!(seen.into_iter().all(|visited| visited), "case {case}");
    }
}
