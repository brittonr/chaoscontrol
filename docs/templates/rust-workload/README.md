# ChaosControl Rust workload template

Copy this directory into a Rust service to add a local ChaosControl harness.

Start with the external harness. Use in-process instrumentation only when the driver cannot observe an important service invariant.

You can copy the template manually or use the scaffold app, which also writes a `chaoscontrol-scaffold.json` manifest containing the exact local dry-run, report, assertion-quality, VM campaign, and promotion-boundary commands:

```bash
nix run .#scaffold-rust-workload -- ./chaos-workload my-service
```

## 1. Wire the SDK

Set the `chaoscontrol-sdk` dependency in `Cargo.toml`.

Assertion catalog macros also require `linkme` in the downstream crate. The registration attribute expands at the call site.

This repository uses a path dependency. Downstream projects must use an accepted pinned revision or registry version.

## 2. Run the local instrumentation smoke

```bash
CHAOSCONTROL_SDK_LOCAL_OUTPUT=/tmp/my-service.sdk.jsonl \
  cargo run --bin my-service-chaos-workload

summarize-sdk-local-output \
  --input /tmp/my-service.sdk.jsonl \
  --output /tmp/my-service.local-report.json

check-sdk-assertion-quality \
  --input /tmp/my-service.local-report.json
```

Review `adoption_tracks`, `assertion_coverage`, `unobserved_assertions`, `sometimes_without_success`, and `reachable_without_hit` before promoting to a VM campaign. The assertion-quality gate fails on missing setup lifecycle, uncategorized assertions, unobserved/reachability assertions, sometimes assertions without success, and failing ordinary assertions. This report is instrumentation evidence only: `replay_evidence` is always `false`.

## 3. Optional advanced in-process instrumentation

If public APIs or the workload driver cannot see an important invariant, move a small assertion into service code behind an explicit gate:

```bash
CHAOSCONTROL_SDK_LOCAL_OUTPUT=/tmp/my-service.in-process.sdk.jsonl \
  cargo run --features chaoscontrol-in-process --bin my-service-chaos-workload

summarize-sdk-local-output \
  --input /tmp/my-service.in-process.sdk.jsonl \
  --output /tmp/my-service.in-process.local-report.json

check-sdk-assertion-quality \
  --input /tmp/my-service.in-process.local-report.json
```

The template tags these observations with `adoption_track = in-process-service`.

Keep this feature optional. Default and production builds must not require service-internal SDK calls.

Move in-process when:

- Important invariants are not visible through public APIs.
- Bugs depend on internal state transitions or timing.
- External harness coverage cannot guide the next assertion.

## 4. Promote only after the local report is useful

Once setup, adoption track labels, and assertion coverage are visible, package the workload as a guest/initrd and run a bounded campaign:

```bash
nix run .#rust-workload-local-report -- /tmp/cc-rust-workload-local
nix run .#explore-rust-workload -- /tmp/cc-rust-workload-vm
```

Promotion checklist:

1. Local dry-run JSONL exists and `summarize-sdk-local-output` produced `report.json`.
2. `check-sdk-assertion-quality --input report.json` passes.
3. Bounded VM campaign output exists with `evidence-classification.json`.
4. Accepted replay proof is claimed only after exported bug evidence, snapshot artifact validation, and a reproduced `snapshot_backed_reproduced` replay verdict.
