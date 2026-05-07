use chaoscontrol_sdk::prelude::*;
use serde_json::json;

const WORKLOAD: &str = "my-service";

fn main() {
    let workload = WorkloadHarness::new(WORKLOAD);
    workload.init();

    // Replace this with service/cluster setup that should complete before the
    // workload starts making assertions.
    workload.setup_complete(json!({
        "nodes": 1,
        "template": "docs/templates/rust-workload",
    }));

    workload.scenario("write/read survives restart", || {
        let action = random_choice(3);

        cc_assert_always_category!(
            WORKLOAD,
            "invariant",
            action < 3,
            "scheduler choice remains in range"
        );

        // Replace the condition with a real operation success signal. A local
        // dry-run should show whether this sometimes assertion ever succeeds.
        cc_assert_sometimes_category!(WORKLOAD, "operation", action == 0, "write succeeds");

        if action == 1 {
            cc_assert_reachable_category!(WORKLOAD, "branch", "read path exercised");
        }
    });

    send_event(
        "workload_done",
        &json!({
            "workload": WORKLOAD,
            "evidence_class": "instrumentation-only until VM/replay rails run",
        }),
    );
}
