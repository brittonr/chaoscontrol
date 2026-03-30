//! Property-based tests for FaultSchedule.
//!
//! Key properties:
//! - All faults are eventually drained when polled at time >= max.
//! - drain_due never returns faults scheduled in the future.
//! - drain_due returns faults in time order.
//! - Snapshot/restore preserves cursor position exactly.
//! - subset() produces valid subschedule.

use chaoscontrol_fault::faults::Fault;
use chaoscontrol_fault::schedule::{FaultSchedule, FaultScheduleBuilder};
use hegel::generators::*;
use hegel::TestCase;

fn fault_times() -> impl hegel::Generator<u64> {
    integers::<u64>().min_value(0).max_value(100_000_000)
}

/// Build a schedule with random fault times.
fn random_schedule(tc: &TestCase, n: usize) -> (FaultSchedule, Vec<u64>) {
    let mut builder = FaultScheduleBuilder::new();
    let mut times = Vec::new();
    for _ in 0..n {
        let t = tc.draw(fault_times());
        times.push(t);
        builder = builder.at_ns(t, Fault::NetworkHeal);
    }
    let schedule = builder.build();
    times.sort();
    (schedule, times)
}

#[hegel::test(test_cases = 300)]
fn all_faults_drained_at_max_time(tc: TestCase) {
    let n = tc.draw(integers::<usize>().min_value(0).max_value(50));
    let (mut schedule, times) = random_schedule(&tc, n);

    let max_time = times.last().copied().unwrap_or(0);
    let faults = schedule.drain_due(max_time);

    assert_eq!(
        faults.len(),
        n,
        "all {} faults should drain at time {}",
        n,
        max_time
    );
    assert_eq!(schedule.remaining(), 0);
}

#[hegel::test(test_cases = 300)]
fn drain_due_never_returns_future_faults(tc: TestCase) {
    let n = tc.draw(integers::<usize>().min_value(1).max_value(30));
    let (mut schedule, _) = random_schedule(&tc, n);

    let query_time = tc.draw(fault_times());
    let faults = schedule.drain_due(query_time);

    for f in &faults {
        assert!(
            f.time_ns <= query_time,
            "fault at time {} returned for query at time {}",
            f.time_ns,
            query_time
        );
    }
}

#[hegel::test(test_cases = 300)]
fn drain_due_returns_in_time_order(tc: TestCase) {
    let n = tc.draw(integers::<usize>().min_value(1).max_value(30));
    let (mut schedule, _) = random_schedule(&tc, n);

    let query_time = tc.draw(
        integers::<u64>()
            .min_value(50_000_000)
            .max_value(200_000_000),
    );
    let faults = schedule.drain_due(query_time);

    for window in faults.windows(2) {
        assert!(
            window[0].time_ns <= window[1].time_ns,
            "faults out of order: {} > {}",
            window[0].time_ns,
            window[1].time_ns
        );
    }
}

#[hegel::test(test_cases = 300)]
fn drain_is_idempotent(tc: TestCase) {
    let n = tc.draw(integers::<usize>().min_value(1).max_value(20));
    let (mut schedule, _) = random_schedule(&tc, n);

    let query_time = tc.draw(fault_times());
    let _first = schedule.drain_due(query_time);
    let second = schedule.drain_due(query_time);

    assert!(
        second.is_empty(),
        "second drain at same time should return nothing (got {} faults)",
        second.len()
    );
}

#[hegel::test(test_cases = 300)]
fn incremental_drain_gets_all_faults(tc: TestCase) {
    let n = tc.draw(integers::<usize>().min_value(1).max_value(30));
    let (mut schedule, _times) = random_schedule(&tc, n);

    // Drain in random increments
    let n_steps = tc.draw(integers::<usize>().min_value(1).max_value(10));
    let mut total_drained = 0;
    let mut last_time = 0u64;

    for _ in 0..n_steps {
        let t = tc.draw(
            integers::<u64>()
                .min_value(last_time)
                .max_value(200_000_000),
        );
        let batch = schedule.drain_due(t);
        for f in &batch {
            assert!(f.time_ns > last_time || total_drained == 0 || f.time_ns <= t);
        }
        total_drained += batch.len();
        last_time = t;
    }

    // Drain everything remaining
    let final_batch = schedule.drain_due(u64::MAX);
    total_drained += final_batch.len();

    assert_eq!(total_drained, n, "incremental drain must get all faults");
}

#[hegel::test(test_cases = 300)]
fn snapshot_restore_preserves_cursor(tc: TestCase) {
    let n = tc.draw(integers::<usize>().min_value(1).max_value(20));
    let (mut schedule, _) = random_schedule(&tc, n);

    // Drain some
    let drain_time = tc.draw(fault_times());
    let _pre_drain = schedule.drain_due(drain_time);
    let remaining_after_drain = schedule.remaining();

    let snap = schedule.snapshot();

    // Drain more
    schedule.drain_due(u64::MAX);
    assert_eq!(schedule.remaining(), 0);

    // Restore
    schedule.restore(&snap);
    assert_eq!(
        schedule.remaining(),
        remaining_after_drain,
        "restore must return to cursor position"
    );

    // Draining again from restored position should yield the same remaining faults
    let post_restore_drain = schedule.drain_due(u64::MAX);
    assert_eq!(post_restore_drain.len(), remaining_after_drain);
}

#[hegel::test(test_cases = 200)]
fn subset_preserves_selected_faults(tc: TestCase) {
    let n = tc.draw(integers::<usize>().min_value(2).max_value(20));
    let (schedule, _) = random_schedule(&tc, n);

    // Pick a random subset of indices
    let indices: Vec<usize> = tc.draw(vecs(booleans()).min_size(n).max_size(n).map(move |flags| {
        flags
            .into_iter()
            .enumerate()
            .filter(|(_, keep)| *keep)
            .map(|(i, _)| i)
            .collect::<Vec<_>>()
    }));

    let sub = schedule.subset(&indices);
    assert_eq!(sub.total(), indices.len());

    // All faults in subset should have times matching the original
    let orig_faults = schedule.faults();
    let sub_faults = sub.faults();

    for (i, sf) in sub_faults.iter().enumerate() {
        let orig_idx = indices[i];
        assert_eq!(
            sf.time_ns, orig_faults[orig_idx].time_ns,
            "subset fault {} time mismatch",
            i
        );
    }
}

#[hegel::test(test_cases = 200)]
fn next_time_tracks_cursor(tc: TestCase) {
    let n = tc.draw(integers::<usize>().min_value(1).max_value(20));
    let (mut schedule, times) = random_schedule(&tc, n);

    // next_time should return the first fault's time
    assert_eq!(schedule.next_time(), Some(times[0]));

    // Drain at distinct sorted times and verify next_time never goes backwards
    let mut prev_next_time = 0u64;
    let mut dedup_times: Vec<u64> = times.clone();
    dedup_times.dedup();

    for &t in &dedup_times {
        schedule.drain_due(t);

        match schedule.next_time() {
            Some(next) => {
                assert!(next > t, "next_time {} should be > drain time {}", next, t);
                assert!(
                    next >= prev_next_time,
                    "next_time went backwards: {} < {}",
                    next,
                    prev_next_time
                );
                prev_next_time = next;
            }
            None => {
                // All faults consumed
                assert_eq!(schedule.remaining(), 0);
            }
        }
    }
}
