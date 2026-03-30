//! Property-based tests for FaultEngine.
//!
//! Key properties:
//! - Same seed → identical random sequence (determinism).
//! - Snapshot/restore → identical continuation (reproducibility).
//! - Override at position N doesn't affect positions > N (RNG sync).
//! - random_choice(n) always returns value in [0, n).
//! - Scheduled faults never fire before setup_complete.
//! - Scheduled faults are delivered in time order.

use chaoscontrol_fault::engine::{EngineConfig, FaultEngine};
use chaoscontrol_fault::faults::Fault;
use chaoscontrol_fault::schedule::FaultScheduleBuilder;
use chaoscontrol_protocol::*;
use hegel::generators::*;
use hegel::TestCase;
use std::collections::BTreeMap;

fn make_random_get_page() -> HypercallPage {
    let mut page = HypercallPage::zeroed();
    page.command = CMD_RANDOM_GET;
    page
}

fn make_random_choice_page(n: u32) -> HypercallPage {
    let mut page = HypercallPage::zeroed();
    page.command = CMD_RANDOM_CHOICE;
    page.id = n;
    page
}

#[hegel::test(test_cases = 300)]
fn deterministic_random_sequence(tc: TestCase) {
    let seed = tc.draw(integers::<u64>());
    let n_calls = tc.draw(integers::<usize>().min_value(1).max_value(200));

    let mut e1 = FaultEngine::new(EngineConfig {
        seed,
        ..Default::default()
    });
    let mut e2 = FaultEngine::new(EngineConfig {
        seed,
        ..Default::default()
    });
    e1.begin_run();
    e2.begin_run();

    let page = make_random_get_page();
    for i in 0..n_calls {
        let (v1, s1) = e1.handle_hypercall(&page);
        let (v2, s2) = e2.handle_hypercall(&page);
        assert_eq!(v1, v2, "mismatch at call {}", i);
        assert_eq!(s1, STATUS_OK);
        assert_eq!(s2, STATUS_OK);
    }
}

#[hegel::test(test_cases = 300)]
fn snapshot_restore_continues_sequence(tc: TestCase) {
    let seed = tc.draw(integers::<u64>());
    let pre_calls = tc.draw(integers::<usize>().min_value(1).max_value(100));
    let post_calls = tc.draw(integers::<usize>().min_value(1).max_value(100));

    let mut engine = FaultEngine::new(EngineConfig {
        seed,
        ..Default::default()
    });
    engine.begin_run();

    let page = make_random_get_page();

    // Advance to snapshot point
    for _ in 0..pre_calls {
        engine.handle_hypercall(&page);
    }
    let snap = engine.snapshot();

    // Collect post-snapshot values
    let mut orig_vals = Vec::new();
    for _ in 0..post_calls {
        let (v, _) = engine.handle_hypercall(&page);
        orig_vals.push(v);
    }

    // Restore and collect same range
    engine.restore(&snap);
    let mut restored_vals = Vec::new();
    for _ in 0..post_calls {
        let (v, _) = engine.handle_hypercall(&page);
        restored_vals.push(v);
    }

    assert_eq!(orig_vals, restored_vals);
}

#[hegel::test(test_cases = 300)]
fn random_choice_always_in_range(tc: TestCase) {
    let seed = tc.draw(integers::<u64>());
    let n = tc.draw(integers::<u32>().min_value(1).max_value(10000));
    let n_calls = tc.draw(integers::<usize>().min_value(1).max_value(100));

    let mut engine = FaultEngine::new(EngineConfig {
        seed,
        ..Default::default()
    });
    engine.begin_run();

    let page = make_random_choice_page(n);
    for i in 0..n_calls {
        let (val, status) = engine.handle_hypercall(&page);
        assert_eq!(status, STATUS_OK);
        assert!(
            val < n as u64,
            "call {}: random_choice({}) returned {} (out of range)",
            i,
            n,
            val
        );
    }
}

#[hegel::test(test_cases = 200)]
fn random_choice_of_1_always_zero(tc: TestCase) {
    let seed = tc.draw(integers::<u64>());
    let n_calls = tc.draw(integers::<usize>().min_value(1).max_value(50));

    let mut engine = FaultEngine::new(EngineConfig {
        seed,
        ..Default::default()
    });
    engine.begin_run();

    let page = make_random_choice_page(1);
    for _ in 0..n_calls {
        let (val, _) = engine.handle_hypercall(&page);
        assert_eq!(val, 0, "random_choice(1) must always return 0");
    }
}

#[hegel::test(test_cases = 200)]
fn override_preserves_rng_sync(tc: TestCase) {
    let seed = tc.draw(integers::<u64>());
    let override_pos = tc.draw(integers::<u64>().min_value(0).max_value(20));
    let override_val = tc.draw(integers::<u64>());
    let total_calls = tc.draw(
        integers::<usize>()
            .min_value((override_pos as usize) + 5)
            .max_value(50),
    );

    // Engine A: uses override at position override_pos
    let mut a = FaultEngine::new(EngineConfig {
        seed,
        ..Default::default()
    });
    a.begin_run();
    let mut overrides = BTreeMap::new();
    overrides.insert(override_pos, override_val);
    a.set_random_overrides(overrides);

    // Engine B: no overrides
    let mut b = FaultEngine::new(EngineConfig {
        seed,
        ..Default::default()
    });
    b.begin_run();

    let page = make_random_get_page();
    let mut vals_a = Vec::new();
    let mut vals_b = Vec::new();

    for _ in 0..total_calls {
        let (va, _) = a.handle_hypercall(&page);
        let (vb, _) = b.handle_hypercall(&page);
        vals_a.push(va);
        vals_b.push(vb);
    }

    // At override position: values should differ (unless they happen to match)
    // After override position: values should be identical (RNG in sync)
    for i in (override_pos as usize + 1)..total_calls {
        assert_eq!(
            vals_a[i], vals_b[i],
            "position {}: RNG should be in sync after override at {}",
            i, override_pos
        );
    }
}

#[hegel::test(test_cases = 200)]
fn faults_never_fire_before_setup_complete(tc: TestCase) {
    let seed = tc.draw(integers::<u64>());
    let n_faults = tc.draw(integers::<usize>().min_value(1).max_value(10));

    let mut builder = FaultScheduleBuilder::new();
    for i in 0..n_faults {
        let time_ns = tc.draw(integers::<u64>().min_value(0).max_value(10_000_000));
        builder = builder.at_ns(time_ns, Fault::ProcessKill { target: i % 3 });
    }
    let schedule = builder.build();

    let mut engine = FaultEngine::new(EngineConfig {
        seed,
        schedule: Some(schedule),
        num_vms: 3,
        ..Default::default()
    });
    engine.begin_run();

    // Poll at various times without setup_complete — should get nothing
    for _ in 0..20 {
        let time = tc.draw(integers::<u64>().min_value(0).max_value(100_000_000));
        let faults = engine.poll_faults(time);
        assert!(
            faults.is_empty(),
            "faults fired at time {} without setup_complete",
            time
        );
    }
}

#[hegel::test(test_cases = 200)]
fn choice_history_records_all_calls(tc: TestCase) {
    let seed = tc.draw(integers::<u64>());
    let n_calls = tc.draw(integers::<usize>().min_value(1).max_value(50));

    let mut engine = FaultEngine::new(EngineConfig {
        seed,
        ..Default::default()
    });
    engine.begin_run();

    let mut expected_values = Vec::new();
    for _i in 0..n_calls {
        let use_choice = tc.draw(booleans());
        if use_choice {
            let n = tc.draw(integers::<u32>().min_value(2).max_value(100));
            let page = make_random_choice_page(n);
            let (val, _) = engine.handle_hypercall(&page);
            expected_values.push((n, val));
        } else {
            let page = make_random_get_page();
            let (val, _) = engine.handle_hypercall(&page);
            expected_values.push((0, val));
        }
    }

    let history = engine.drain_choice_history();
    assert_eq!(history.len(), n_calls);

    for (i, record) in history.iter().enumerate() {
        assert_eq!(record.sequence_id, i as u64);
        assert_eq!(record.n_options, expected_values[i].0);
        assert_eq!(record.value, expected_values[i].1);
    }
}
