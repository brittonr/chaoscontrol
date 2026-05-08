## Why

ChaosControl now has a credible local-first Rust workload harness and bounded replay-readiness rail, but the adoption story should not hide the deeper Antithesis-style option: teams may need to instrument the real service process when important invariants are invisible from a black-box harness. Without an explicit advanced path, docs and APIs can either overfit to the non-invasive template or imply that service-internal SDK calls are unsupported.

## What Changes

- **Advanced in-process path**: Define an opt-in path for linking `chaoscontrol-sdk` into service/runtime code rather than only into an external workload driver.
- **Escalation guidance**: Document when to stay on the harness path and when to move assertions, guided randomness, or lifecycle hooks into service internals.
- **Evidence boundaries**: Require reports and readiness output to distinguish harness-local evidence, in-process local evidence, and VM snapshot-backed replay proof.
- **Sample integration**: Add a small downstream-shaped example showing service-internal assertions behind explicit feature/config gates.

## Capabilities

### Modified Capabilities
- `rust-workload-harness`: Adds a supported advanced instrumentation track without changing the default local-first harness path.

## Impact

- **Files**: `docs/`, `docs/templates/rust-workload/`, `chaoscontrol-sdk` examples or fixtures, possible helper scripts for local-output summaries.
- **APIs**: No default production behavior change; any in-process service hooks must be explicit and opt-in.
- **Dependencies**: No new required non-Rust or hosted dependencies.
- **Testing**: Validate with local no-VM smoke, feature-gated sample tests, summary/report checks, and optional VM proof only when runtime artifacts are prepared.
