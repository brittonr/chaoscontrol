//! Downstream-style Rust workload packaged as a ChaosControl guest.
//!
//! This binary intentionally uses the public workload harness surface so the
//! Nix rail exercises the same shape a downstream Rust project should copy.

use chaoscontrol_sdk::prelude::*;
use serde_json::json;

const WORKLOAD: &str = "sample-rust-service";
const ITERATIONS: usize = 12;

fn main() {
    if std::env::var_os("CHAOSCONTROL_SDK_LOCAL_OUTPUT").is_none() {
        guest_init();
    }

    let workload = WorkloadHarness::new(WORKLOAD);
    workload.init();
    workload.setup_complete(json!({
        "nodes": 3,
        "packaging": "nix-initrd",
        "evidence_class": "instrumentation-or-vm-campaign",
    }));

    let mut writes = 0usize;
    let mut reads = 0usize;

    for iteration in 0..ITERATIONS {
        workload.scenario("writes survive failover", || {
            let action = random_choice(3);
            cc_assert_always_category!(
                WORKLOAD,
                "invariant",
                action < 3,
                "choice remains in range"
            );

            if action == 0 {
                writes += 1;
                cc_assert_sometimes_category!(WORKLOAD, "operation", true, "write succeeds");
            }
            if action == 1 {
                reads += 1;
                cc_assert_reachable_category!(WORKLOAD, "branch", "read branch exercised");
            }

            cc_assert_always_category!(
                WORKLOAD,
                "invariant",
                writes + reads <= iteration + 1,
                "operation counters stay bounded"
            );
        });
    }

    cc_assert_sometimes_category!(
        WORKLOAD,
        "operation",
        writes > 0,
        "at least one write succeeds"
    );
    send_event(
        "workload_done",
        &json!({
            "workload": WORKLOAD,
            "iterations": ITERATIONS,
            "writes": writes,
            "reads": reads,
            "evidence_class": "instrumentation-only unless run under VM campaign",
        }),
    );

    if std::env::var_os("CHAOSCONTROL_SDK_LOCAL_OUTPUT").is_none() {
        loop {
            unsafe { libc::pause() };
        }
    }
}
