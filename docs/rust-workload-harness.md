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
- registered/cataloged vs observed/exercised assertions;
- deterministic `assertion_coverage` entries with ID, message, type, category,
  observed hit count, success/failure counts, and observed/unobserved status;
- uncategorized assertions;
- `sometimes` assertions that did not observe success;
- reachable assertions that were not hit;
- local `random_choice` observations.

This dry-run is only an instrumentation check. It does not prove deterministic replay or snapshot-backed reproduction; VM campaign reports must keep those evidence classes separate.

## In-tree sample

- `crates/chaoscontrol-sdk/examples/rust_workload_harness.rs` is a downstream-style example that uses only the public prelude surface.
- `docs/templates/rust-workload/` is the copyable golden path for a downstream service. Start there when evaluating ChaosControl as an Antithesis-style Rust workload harness: run the local smoke first, fix instrumentation gaps from the report, then promote to VM/replay rails only when the assertion surface is useful.

## Nix packaging and one-command rails

The repository also includes a downstream-shaped guest crate at
`crates/chaoscontrol-rust-workload-guest/`. It uses the same public harness
surface, then Nix packages it as a static guest binary and initrd:

```bash
nix build .#guest-rust-workload
nix build .#initrd-rust-workload
```

For a local instrumentation check, run:

```bash
nix run .#rust-workload-local-report -- /tmp/cc-rust-workload-local
```

That writes:

- `sdk.jsonl` — raw local SDK fallback events;
- `report.json` — summarized setup/assertion/randomness coverage with
  `evidence_class = instrumentation-dry-run` and `replay_evidence = false`.

For a bounded VM campaign against the packaged initrd, run:

```bash
nix run .#explore-rust-workload -- /tmp/cc-rust-workload-vm
```

That writes the explorer output directory plus `evidence-classification.json`.
The campaign output is VM execution evidence; standalone replay proof remains a
separate classification that must be backed by replay/minimization artifacts.

For the accepted snapshot-backed replay proof rail, run the dedicated dogfood
wrapper:

```bash
nix run .#rust-workload-accepted-verdict-dogfood -- \
  --output dogfood-results/rust-workload-accepted-verdict-dogfood-<timestamp>
```

This reuses `scripts/accepted-snapshot-verdict-dogfood.py` with the
KCOV-enabled kernel, `.#initrd-rust-workload`, assertion ID `1414213562`, and
`rust_workload_bug=snapshot_replay_probe`. It is intentionally a slower VM and
replay rail: if the KCOV kernel is not cached, Nix may build Linux before the
run starts. Acceptance still requires filtered `export-bugs`, a valid persisted
parent snapshot artifact, and a replay verdict with
`replay_class = snapshot_backed_reproduced`.
