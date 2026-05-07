# Rust Workload Harness

ChaosControl's SDK is Rust-first. The workload harness is the thin layer for using it across local Rust projects without repeating VM/setup/report glue.

## Minimal downstream shape

```rust
use chaoscontrol_sdk::prelude::*;

fn main() {
    let workload = WorkloadHarness::new("my-service");
    workload.init();

    start_cluster();
    workload.setup_complete(json!({ "nodes": 3 }));

    workload.scenario("writes survive failover", || {
        let action = random_choice(3);
        cc_assert_always_category!("my-service", "invariant", action < 3, "choice in range");
        cc_assert_sometimes_category!("my-service", "operation", action == 0, "write succeeds");
    });
}
```

The harness does not replace `chaoscontrol_sdk::assert`, `lifecycle`, `guidance`, or `random`; it gives those primitives a repeatable workload/scenario shape.

## Local dry-run

Set `CHAOSCONTROL_SDK_LOCAL_OUTPUT` before first SDK use, then run the workload normally:

```bash
CHAOSCONTROL_SDK_LOCAL_OUTPUT=/tmp/my-service.sdk.jsonl \
  cargo run --bin my-service-chaos-workload
```

Parse the output with `LocalDryRunReport::from_path` to inspect:

- whether `setup_complete` was emitted;
- cataloged vs exercised assertions;
- uncategorized assertions;
- `sometimes` assertions that did not observe success;
- reachable assertions that were not hit;
- local `random_choice` observations.

This dry-run is only an instrumentation check. It does not prove deterministic replay or snapshot-backed reproduction; VM campaign reports must keep those evidence classes separate.

## In-tree sample

`crates/chaoscontrol-sdk/examples/rust_workload_harness.rs` is a downstream-style example that uses only the public prelude surface.
