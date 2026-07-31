//! Deterministic sweep tests for FaultEngine.
//!
//! These cover the same invariants as the prior property tests without pulling a
//! proc-macro property-test dependency into the audit surface.

use chaoscontrol_fault::engine::{EngineConfig, FaultEngine};
use chaoscontrol_fault::faults::Fault;
use chaoscontrol_fault::schedule::FaultScheduleBuilder;
use chaoscontrol_protocol::*;
use std::collections::BTreeMap;

const WIDE_CASES: u64 = 300;
const NORMAL_CASES: u64 = 200;

#[derive(Clone)]
struct DeterministicCase {
    state: u64,
}

impl DeterministicCase {
    fn new(index: u64) -> Self {
        Self {
            state: index ^ 0x9e37_79b9_7f4a_7c15,
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

    fn u32(&mut self, min: u32, max: u32) -> u32 {
        min + (self.next() as u32 % (max - min + 1))
    }

    fn bool(&mut self) -> bool {
        self.next() & 1 == 1
    }
}

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

#[test]
fn deterministic_random_sequence() {
    for case in 0..WIDE_CASES {
        let mut tc = DeterministicCase::new(case);
        let seed = tc.next();
        let n_calls = tc.usize(1, 200);

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
            assert_eq!(v1, v2, "case {case}: mismatch at call {i}");
            assert_eq!(s1, STATUS_OK);
            assert_eq!(s2, STATUS_OK);
        }
    }
}

#[test]
fn snapshot_restore_continues_sequence() {
    for case in 0..WIDE_CASES {
        let mut tc = DeterministicCase::new(case);
        let seed = tc.next();
        let pre_calls = tc.usize(1, 100);
        let post_calls = tc.usize(1, 100);

        let mut engine = FaultEngine::new(EngineConfig {
            seed,
            ..Default::default()
        });
        engine.begin_run();

        let page = make_random_get_page();
        for _ in 0..pre_calls {
            engine.handle_hypercall(&page);
        }
        let snap = engine.snapshot();

        let mut orig_vals = Vec::new();
        for _ in 0..post_calls {
            let (v, _) = engine.handle_hypercall(&page);
            orig_vals.push(v);
        }

        engine.restore(&snap).expect("restore engine");
        let mut restored_vals = Vec::new();
        for _ in 0..post_calls {
            let (v, _) = engine.handle_hypercall(&page);
            restored_vals.push(v);
        }

        assert_eq!(orig_vals, restored_vals, "case {case}");
    }
}

#[test]
fn random_choice_always_in_range() {
    for case in 0..WIDE_CASES {
        let mut tc = DeterministicCase::new(case);
        let seed = tc.next();
        let n = tc.u32(1, 10_000);
        let n_calls = tc.usize(1, 100);

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
                "case {case}, call {i}: random_choice({n}) returned {val}"
            );
        }
    }
}

#[test]
fn random_choice_of_1_always_zero() {
    for case in 0..NORMAL_CASES {
        let mut tc = DeterministicCase::new(case);
        let seed = tc.next();
        let n_calls = tc.usize(1, 50);

        let mut engine = FaultEngine::new(EngineConfig {
            seed,
            ..Default::default()
        });
        engine.begin_run();

        let page = make_random_choice_page(1);
        for _ in 0..n_calls {
            let (val, _) = engine.handle_hypercall(&page);
            assert_eq!(val, 0, "case {case}: random_choice(1) must return 0");
        }
    }
}

#[test]
fn override_preserves_rng_sync() {
    for case in 0..NORMAL_CASES {
        let mut tc = DeterministicCase::new(case);
        let seed = tc.next();
        let override_pos = tc.u64(0, 20);
        let override_val = tc.next();
        let total_calls = tc.usize((override_pos as usize) + 5, 50);

        let mut a = FaultEngine::new(EngineConfig {
            seed,
            ..Default::default()
        });
        a.begin_run();
        let mut overrides = BTreeMap::new();
        overrides.insert(override_pos, override_val);
        a.set_random_overrides(overrides);

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

        for i in (override_pos as usize + 1)..total_calls {
            assert_eq!(
                vals_a[i], vals_b[i],
                "case {case}, position {i}: RNG should sync after override at {override_pos}"
            );
        }
    }
}

#[test]
fn faults_never_fire_before_setup_complete() {
    for case in 0..NORMAL_CASES {
        let mut tc = DeterministicCase::new(case);
        let seed = tc.next();
        let n_faults = tc.usize(1, 10);

        let mut builder = FaultScheduleBuilder::new();
        for i in 0..n_faults {
            let time_ns = tc.u64(0, 10_000_000);
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

        for _ in 0..20 {
            let time = tc.u64(0, 100_000_000);
            let faults = engine.poll_faults(time);
            assert!(
                faults.is_empty(),
                "case {case}: faults fired at time {time} without setup_complete"
            );
        }
    }
}

#[test]
fn choice_history_records_all_calls() {
    for case in 0..NORMAL_CASES {
        let mut tc = DeterministicCase::new(case);
        let seed = tc.next();
        let n_calls = tc.usize(1, 50);

        let mut engine = FaultEngine::new(EngineConfig {
            seed,
            ..Default::default()
        });
        engine.begin_run();

        let mut expected_values = Vec::new();
        for _ in 0..n_calls {
            if tc.bool() {
                let n = tc.u32(2, 100);
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
        assert_eq!(history.len(), n_calls, "case {case}");

        for (i, record) in history.iter().enumerate() {
            assert_eq!(record.sequence_id, i as u64);
            assert_eq!(record.n_options, expected_values[i].0);
            assert_eq!(record.value, expected_values[i].1);
        }
    }
}
