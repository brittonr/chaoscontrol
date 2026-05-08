# ChaosControl Rust Workload Template

Copy this directory into a Rust service when you want a local-first ChaosControl harness before spending VM/replay budget. Start with the external harness path; enable the advanced in-process path only when the driver cannot observe important service invariants.

## 1. Wire the SDK

Set the `chaoscontrol-sdk` dependency in `Cargo.toml`. Assertion catalog macros also require `linkme` in the downstream crate because the registration attribute expands at the call site. Inside this repository the template uses a path dependency; downstream projects should use the pinned git/revision or registry source they have accepted.

## 2. Run the local instrumentation smoke

```bash
CHAOSCONTROL_SDK_LOCAL_OUTPUT=/tmp/my-service.sdk.jsonl \
  cargo run --bin my-service-chaos-workload

python /path/to/chaoscontrol/scripts/summarize-sdk-local-output.py \
  --input /tmp/my-service.sdk.jsonl \
  --output /tmp/my-service.local-report.json
```

Review `adoption_tracks`, `assertion_coverage`, `unobserved_assertions`, `sometimes_without_success`, and `reachable_without_hit` before promoting to a VM campaign. This report is instrumentation evidence only: `replay_evidence` is always `false`.

## 3. Optional advanced in-process instrumentation

If public APIs or the workload driver cannot see an important invariant, move a small assertion into service code behind an explicit gate:

```bash
CHAOSCONTROL_SDK_LOCAL_OUTPUT=/tmp/my-service.in-process.sdk.jsonl \
  cargo run --features chaoscontrol-in-process --bin my-service-chaos-workload

python /path/to/chaoscontrol/scripts/summarize-sdk-local-output.py \
  --input /tmp/my-service.in-process.sdk.jsonl \
  --output /tmp/my-service.in-process.local-report.json
```

The template service module tags these observations with `adoption_track = in-process-service`. Keep this opt-in: production/default builds should not require service-internal SDK calls.

Move in-process when:

- important invariants are invisible from public APIs;
- bugs depend on internal state transitions or timing;
- external harness coverage is too shallow to guide the next assertion.

## 4. Promote only after the local report is useful

Once setup, adoption track labels, and assertion coverage are visible, package the workload as a guest/initrd and run a bounded campaign. Accepted replay proof still requires exported bug evidence plus a reproduced snapshot-backed replay verdict.
