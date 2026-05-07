//! Downstream-style Rust workload harness example.
//!
//! Run locally with:
//! `CHAOSCONTROL_SDK_LOCAL_OUTPUT=/tmp/cc-sdk.jsonl cargo run -p chaoscontrol-sdk --example rust_workload_harness`

use chaoscontrol_sdk::prelude::*;

fn main() {
    let workload = WorkloadHarness::new("sample-rust-service");
    workload.init();
    workload.setup_complete(json!({ "nodes": 3 }));

    workload.scenario("writes survive failover", || {
        let action = random_choice(3);
        cc_assert_always_category!(
            "sample-rust-service",
            "invariant",
            action < 3,
            "choice in range"
        );
        cc_assert_sometimes_category!(
            "sample-rust-service",
            "operation",
            action == 0,
            "write succeeds"
        );
        if action == 1 {
            cc_assert_reachable_category!("sample-rust-service", "branch", "read branch exercised");
        }
    });
}
