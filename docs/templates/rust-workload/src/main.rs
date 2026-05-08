mod service;

use chaoscontrol_sdk::prelude::*;
use serde_json::json;
use service::KeyValueService;

const WORKLOAD: &str = "my-service";

fn main() {
    let workload = WorkloadHarness::new(WORKLOAD);
    workload.init();

    // Replace this with service/cluster setup that should complete before the
    // workload starts making assertions. The adoption track marks this as the
    // non-invasive external harness path.
    workload.setup_complete(json!({
        "nodes": 1,
        "template": "docs/templates/rust-workload",
        "adoption_track": "external-harness",
    }));

    workload.scenario("write/read survives restart", || {
        let action = random_choice(3);
        let mut service = KeyValueService::start();
        let write_ok = service.write("account-1", "balance=10");
        let read_ok = service.read_after_restart();

        let driver_details = json!({
            "category": "workload-driver",
            "adoption_track": "external-harness",
        });

        cc_assert_always_category!(
            WORKLOAD,
            "workload-driver",
            action < 3,
            "scheduler choice remains in range",
            &driver_details,
        );

        cc_assert_sometimes_category!(
            WORKLOAD,
            "operation",
            write_ok,
            "write succeeds",
            &driver_details,
        );

        cc_assert_sometimes_category!(
            WORKLOAD,
            "operation",
            read_ok && service.committed_writes() > 0,
            "read after restart succeeds",
            &driver_details,
        );

        if action == 1 {
            cc_assert_reachable_category!(
                WORKLOAD,
                "branch",
                "read path exercised",
                &driver_details,
            );
        }
    });

    send_event(
        "workload_done",
        &json!({
            "workload": WORKLOAD,
            "adoption_track": "external-harness",
            "advanced_in_process_enabled": cfg!(feature = "chaoscontrol-in-process"),
            "evidence_class": "instrumentation-only until VM/replay rails run",
        }),
    );
}
