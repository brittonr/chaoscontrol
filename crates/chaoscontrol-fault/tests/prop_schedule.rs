//! Deterministic sweep tests for FaultSchedule.
//!
//! These cover the schedule invariants without pulling a proc-macro property-test
//! dependency into the dependency-audit surface.

use chaoscontrol_fault::faults::Fault;
use chaoscontrol_fault::schedule::{FaultSchedule, FaultScheduleBuilder};

const WIDE_CASES: u64 = 300;
const NORMAL_CASES: u64 = 200;

#[derive(Clone)]
struct DeterministicCase {
    state: u64,
}

impl DeterministicCase {
    fn new(index: u64) -> Self {
        Self {
            state: index ^ 0xd1b5_4a32_d192_ed03,
        }
    }

    fn next(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(2_862_933_555_777_941_757)
            .wrapping_add(3_037_000_493);
        self.state
    }

    fn u64(&mut self, min: u64, max: u64) -> u64 {
        min + (self.next() % (max - min + 1))
    }

    fn usize(&mut self, min: usize, max: usize) -> usize {
        min + (self.next() as usize % (max - min + 1))
    }

    fn bool(&mut self) -> bool {
        self.next() & 1 == 1
    }
}

fn fault_time(tc: &mut DeterministicCase) -> u64 {
    tc.u64(0, 100_000_000)
}

fn random_schedule(tc: &mut DeterministicCase, n: usize) -> (FaultSchedule, Vec<u64>) {
    let mut builder = FaultScheduleBuilder::new();
    let mut times = Vec::new();
    for _ in 0..n {
        let t = fault_time(tc);
        times.push(t);
        builder = builder.at_ns(t, Fault::NetworkHeal);
    }
    let schedule = builder.build();
    times.sort();
    (schedule, times)
}

#[test]
fn all_faults_drained_at_max_time() {
    for case in 0..WIDE_CASES {
        let mut tc = DeterministicCase::new(case);
        let n = tc.usize(0, 50);
        let (mut schedule, times) = random_schedule(&mut tc, n);

        let max_time = times.last().copied().unwrap_or(0);
        let faults = schedule.drain_due(max_time);

        assert_eq!(
            faults.len(),
            n,
            "case {case}: all {n} faults should drain at time {max_time}"
        );
        assert_eq!(schedule.remaining(), 0);
    }
}

#[test]
fn drain_due_never_returns_future_faults() {
    for case in 0..WIDE_CASES {
        let mut tc = DeterministicCase::new(case);
        let n = tc.usize(1, 30);
        let (mut schedule, _) = random_schedule(&mut tc, n);

        let query_time = fault_time(&mut tc);
        let faults = schedule.drain_due(query_time);

        for f in &faults {
            assert!(
                f.time_ns <= query_time,
                "case {case}: fault at time {} returned for query at time {query_time}",
                f.time_ns
            );
        }
    }
}

#[test]
fn drain_due_returns_in_time_order() {
    for case in 0..WIDE_CASES {
        let mut tc = DeterministicCase::new(case);
        let n = tc.usize(1, 30);
        let (mut schedule, _) = random_schedule(&mut tc, n);

        let query_time = tc.u64(50_000_000, 200_000_000);
        let faults = schedule.drain_due(query_time);

        for window in faults.windows(2) {
            assert!(
                window[0].time_ns <= window[1].time_ns,
                "case {case}: faults out of order: {} > {}",
                window[0].time_ns,
                window[1].time_ns
            );
        }
    }
}

#[test]
fn drain_is_idempotent() {
    for case in 0..WIDE_CASES {
        let mut tc = DeterministicCase::new(case);
        let n = tc.usize(1, 20);
        let (mut schedule, _) = random_schedule(&mut tc, n);

        let query_time = fault_time(&mut tc);
        let _first = schedule.drain_due(query_time);
        let second = schedule.drain_due(query_time);

        assert!(
            second.is_empty(),
            "case {case}: second drain at same time returned {} faults",
            second.len()
        );
    }
}

#[test]
fn incremental_drain_gets_all_faults() {
    for case in 0..WIDE_CASES {
        let mut tc = DeterministicCase::new(case);
        let n = tc.usize(1, 30);
        let (mut schedule, _times) = random_schedule(&mut tc, n);

        let n_steps = tc.usize(1, 10);
        let mut total_drained = 0;
        let mut last_time = 0u64;

        for _ in 0..n_steps {
            let t = tc.u64(last_time, 200_000_000);
            let batch = schedule.drain_due(t);
            for f in &batch {
                assert!(f.time_ns > last_time || total_drained == 0 || f.time_ns <= t);
            }
            total_drained += batch.len();
            last_time = t;
        }

        let final_batch = schedule.drain_due(u64::MAX);
        total_drained += final_batch.len();

        assert_eq!(
            total_drained, n,
            "case {case}: incremental drain missed faults"
        );
    }
}

#[test]
fn snapshot_restore_preserves_cursor() {
    for case in 0..WIDE_CASES {
        let mut tc = DeterministicCase::new(case);
        let n = tc.usize(1, 20);
        let (mut schedule, _) = random_schedule(&mut tc, n);

        let drain_time = fault_time(&mut tc);
        let _pre_drain = schedule.drain_due(drain_time);
        let remaining_after_drain = schedule.remaining();

        let snap = schedule.snapshot();

        schedule.drain_due(u64::MAX);
        assert_eq!(schedule.remaining(), 0);

        schedule.restore(&snap);
        assert_eq!(
            schedule.remaining(),
            remaining_after_drain,
            "case {case}: restore must return to cursor position"
        );

        let post_restore_drain = schedule.drain_due(u64::MAX);
        assert_eq!(post_restore_drain.len(), remaining_after_drain);
    }
}

#[test]
fn subset_preserves_selected_faults() {
    for case in 0..NORMAL_CASES {
        let mut tc = DeterministicCase::new(case);
        let n = tc.usize(2, 20);
        let (schedule, _) = random_schedule(&mut tc, n);

        let indices: Vec<usize> = (0..n).filter(|_| tc.bool()).collect();
        let sub = schedule
            .subset(&indices)
            .expect("generated indices are valid");
        assert_eq!(sub.total(), indices.len(), "case {case}");

        let orig_faults = schedule.faults();
        let sub_faults = sub.faults();

        for (i, sf) in sub_faults.iter().enumerate() {
            let orig_idx = indices[i];
            assert_eq!(
                sf.time_ns, orig_faults[orig_idx].time_ns,
                "case {case}: subset fault {i} time mismatch"
            );
        }
    }
}

#[test]
fn next_time_tracks_cursor() {
    for case in 0..NORMAL_CASES {
        let mut tc = DeterministicCase::new(case);
        let n = tc.usize(1, 20);
        let (mut schedule, times) = random_schedule(&mut tc, n);

        assert_eq!(schedule.next_time(), Some(times[0]));

        let mut prev_next_time = 0u64;
        let mut dedup_times: Vec<u64> = times.clone();
        dedup_times.dedup();

        for &t in &dedup_times {
            schedule.drain_due(t);

            match schedule.next_time() {
                Some(next) => {
                    assert!(next > t, "case {case}: next_time {next} should be > {t}");
                    assert!(
                        next >= prev_next_time,
                        "case {case}: next_time went backwards: {next} < {prev_next_time}"
                    );
                    prev_next_time = next;
                }
                None => assert_eq!(schedule.remaining(), 0),
            }
        }
    }
}
