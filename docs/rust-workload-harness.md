# Rust Workload Harness

ChaosControl's SDK is Rust-only for the current product scope. The workload harness is the supported path for local Rust projects without repeating VM/setup/report glue; Go, Java, Python, C, and other SDKs are not current readiness blockers.

## Minimal downstream shape

```rust
use chaoscontrol_sdk::prelude::*;

fn main() {
    let workload = WorkloadHarness::new("my-service");
    workload.init();

    start_cluster();
    workload.setup_complete(json!({ "nodes": 3 }));

    workload.scenario("writes survive failover", || {
        let action = random_choice_index(3);
        cc_assert_always_category!("my-service", "invariant", action < 3, "choice in range");
        cc_assert_sometimes_category!("my-service", "operation", action == 0, "write succeeds");
    });
}
```

The harness does not replace `chaoscontrol_sdk::assert`, `lifecycle`, `guidance`, or `random`; it gives those primitives a repeatable workload/scenario shape. This is the default non-invasive path: drive the service externally, tag local observations as `external-harness`, and inspect coverage before touching service internals.

Assertion and guidance macros register through the SDK-owned `linkme` dependency;
downstream crates do not need to declare it, even when renaming `chaoscontrol-sdk`
in their manifest. Disabling `full` still requires no catalog registration dependency.
Use the category argument to category-specific macros as the source of assertion
category; shared detail objects must not repeat a conflicting `category` field.

Stable-identity assertions accept borrowed `serde::Serialize` details just like
automatic assertions: use a typed diagnostic struct, `&str` or a slice directly
instead of allocating a `serde_json::Value` first. All four stable assertion
kinds share the bounded serializer. Serialization runs only for an accepted,
emitted outcome; sparse sampling skips it for repeated outcomes. With `full`
disabled, the same borrowed calls require no serialization trait and emit nothing.

Third-party adapters using `assert_raw` must register every raw site before any
initialization trigger. For `AssertType::Reachability`, `must_hit=true` identifies
a required reachable path and `must_hit=false` identifies a forbidden path.
The declaration can use `condition=false` and `hit=false`; later observations
retain the same identity regardless of that placeholder. If calling
`AssertType::to_kind` directly, pass `must_hit`, not the observed condition.
Keep the complete declaration descriptor and class/function/display/must-hit
metadata identical on every call; only `condition`, `hit` and diagnostic details
vary. Changed metadata for the same ID is rejected without emitting or consuming
the original site's sparse samples. Exact duplicates remain usable. Distinct
runtime raw string IDs are independent even when their historical compact hashes
collide: their descriptors omit compatibility aliases and their observations use
the existing stable-key/fingerprint-bound transport. Compiled macro aliases are
unchanged. Regenerate runtime raw JSONL and derived reports after upgrading;
runtime raw fingerprints/catalog tokens change at this cutover, not raw output IDs.

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
- local `random_choice_index` observations;
- `adoption_tracks` / `instrumentation_sources`, so external harness observations and opt-in in-process service assertions are not conflated.

In the SDK diagnostic report, `failed_assertions` counts observed
always/unreachable violation hits, not distinct failed properties or every false
condition. False `sometimes` attempts stay in each site's `failure_count`; an
eventual true observation clears the progress gap. Required or unknown-metadata
sites without success still appear in `sometimes_without_success` and `gaps()`,
even when the observed violation counter is zero. An explicitly optional
(`must_hit=false`) sometimes site needs success only after execution. Quiet
optional reachability sites (`Unreachable`) do not enter `reachable_without_hit`;
executed forbidden paths still count as violations. All zero-hit sites remain
in `unobserved_assertions` and coverage rows, including optional ones: that list
is an observation inventory, not a failed-property verdict. Malformed or
conflicting explicit `must_hit` metadata is rejected. Inspect progress gaps as
well as the violation counter.

The strict evidence report separately derives failed-property verdicts and can
therefore report a failed never-successful property when the SDK diagnostic
counter is zero. The SDK diagnostic parser remains non-collision-safe; do not
use its counters as promotion authority instead of the strict quality gate.

The strict local quality gate honors admitted `must_hit=false`: an unexecuted
`AlwaysOrUnreachable` property keeps its registered identity, zero counts and
`observed=false`, but does not block as unobserved. Required or unknown-metadata
sites still block, and any observed optional safety violation still fails.
This does not exempt missing setup, uncategorized sites or invalid identity and
metadata. Rebuild older derived reports that listed a passing optional site as
an observation blocker; report validation recomputes the corrected lists.

This dry-run is only an instrumentation check. It does not prove deterministic replay or snapshot-backed reproduction; VM campaign reports must keep those evidence classes separate.

## Two-track adoption

1. **Default external harness** — keep service code unchanged, drive public APIs from a workload crate, and tag local observations as `external-harness`. This is the first-hour path.
2. **Advanced in-process instrumentation** — when important invariants are invisible from public APIs, place selected SDK assertions or lifecycle hooks inside service code behind explicit feature/config gates and tag those observations as `in-process-service`.

In-process local evidence means the assertion is closer to the invariant source; it still is not replay proof. Accepted replay remains gated by VM campaign artifacts plus a `snapshot_backed_reproduced` replay verdict.

## In-tree sample

- `crates/chaoscontrol-sdk/examples/rust_workload_harness.rs` is a downstream-style example that uses only the public prelude surface.
- `docs/templates/rust-workload/` is the copyable golden path for a downstream service. Start there when evaluating ChaosControl as an Antithesis-style Rust workload harness: run the local smoke first, optionally enable `--features chaoscontrol-in-process` when driver coverage is too shallow, fix instrumentation gaps from the report, then promote to VM/replay rails only when the assertion surface is useful.

To generate a downstream copy plus an explicit command manifest:

```bash
nix run .#scaffold-rust-workload -- ./chaos-workload my-service
```

The scaffold writes `chaoscontrol-scaffold.json` with the local dry-run, local report, `check-sdk-assertion-quality`, bounded VM campaign, and promotion-boundary commands so users do not have to infer the evidence classes.

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
- `assertion-quality.json` — the deterministic pre-VM quality gate result. The
  gate fails on missing setup lifecycle, uncategorized assertions,
  unobserved/reachability assertions, sometimes assertions without success, or
  failing ordinary assertions, and still does not claim replay evidence.

For a bounded VM campaign against the packaged initrd, run:

```bash
nix run .#explore-rust-workload -- /tmp/cc-rust-workload-vm
```

That writes the explorer output directory plus `evidence-classification.json`.
The campaign output is VM execution evidence; standalone replay proof remains a
separate classification that must be backed by replay/minimization artifacts.

For the one-command bounded onboarding classification, run:

```bash
nix run .#fresh-rust-workload-proof -- \
  --scaffold /tmp/my-service \
  --output /tmp/my-service-proof \
  --name my-service
```

The command creates and builds the scaffold. It then runs the packaged,
downstream-shaped Rust KVM workload through replay classification. The KVM
result applies only to that packaged cohort. It does not prove arbitrary
scaffold code.

For the accepted snapshot-backed replay proof rail, run the dedicated dogfood
wrapper:

```bash
nix run .#rust-workload-accepted-verdict-dogfood -- \
  --output dogfood-results/rust-workload-accepted-verdict-dogfood-<timestamp>
```

This reuses `scripts/accepted-snapshot-verdict-dogfood.py` with the
KCOV-enabled kernel, `.#initrd-rust-workload`, assertion alias `3143219316`, and
`rust_workload_bug=snapshot_replay_probe`. It is intentionally a slower VM and
replay rail: if the KCOV kernel is not cached, Nix may build Linux before the
run starts. Acceptance still requires filtered `export-bugs`, exact schema-v2 assertion
identity, the current CBOR snapshot format, a valid persisted parent snapshot,
a linked proof receipt, and `replay_class = snapshot_backed_reproduced`.
