## Context

The default Rust workload harness should remain the low-friction adoption route: a downstream project can drive its service externally, collect local SDK output, and later opt into VM replay proof. That does not cover every useful invariant. Some correctness properties live inside service state machines, storage transitions, queue boundaries, or concurrency handoffs that a black-box driver can only infer indirectly.

Antithesis-style adoption often becomes more valuable once instrumentation is close to the invariant source. ChaosControl should expose that as an advanced track while preserving the non-invasive golden path as the default.

## Goals / Non-Goals

**Goals:**
- Define a supported advanced path for in-process Rust service instrumentation.
- Keep all service-internal instrumentation explicit, feature/config gated, and locally inspectable.
- Make escalation criteria clear: start with a harness, move in-process only when needed.
- Preserve evidence labels so in-process local output is not overclaimed as snapshot-backed replay proof.

**Non-Goals:**
- No multi-language SDK parity.
- No Docker/Kubernetes or hosted UI workflow.
- No requirement that production builds carry ChaosControl instrumentation by default.
- No claim that in-process local output alone proves deterministic replay.

## Decisions

### 1. Keep harness as default, in-process as advanced

**Choice:** Documentation and templates will present two tracks: a default external harness path and an advanced in-process instrumentation path.

**Rationale:** The harness path reduces adoption friction. The in-process path gives teams a clear escalation route when invariants are otherwise unobservable.

**Alternative:** Make in-process instrumentation the primary story. Rejected because it increases first-use cost and makes ChaosControl feel invasive before users trust the tool.

### 2. Require explicit instrumentation gates

**Choice:** In-process examples must place SDK calls behind explicit Cargo features, cfgs, or runtime configuration documented by the sample.

**Rationale:** Teams need confidence that instrumentation is intentional and auditable. The project should not suggest silently changing production behavior.

**Alternative:** Always link and emit SDK calls unconditionally. Rejected because it blurs test-only and production behavior boundaries.

### 3. Report adoption track and evidence class separately

**Choice:** Local reports and readiness summaries must identify whether observations came from an external harness or in-process instrumentation, and must separately classify replay evidence.

**Rationale:** In-process instrumentation can improve bug depth, but local output still is not accepted replay proof unless the VM/export/reproduce rail produces `snapshot_backed_reproduced` evidence.

**Alternative:** Merge all observations into a single readiness status. Rejected because it would invite Antithesis-parity overclaims.

### 4. Start with a tiny downstream-shaped sample

**Choice:** The first implementation should add a minimal sample service/workload pair that demonstrates feature-gated internal assertions plus external driving.

**Rationale:** A small sample can prove API ergonomics and report shape without requiring a broad framework or new CLI.

**Alternative:** Build a full `chaoscontrol init` generator first. Rejected as higher churn and premature before the contract is proven.

## Risks / Trade-offs

**Instrumentation leakage** → Mitigate with feature-gated examples, docs that state production behavior is opt-in, and tests that exercise disabled/default builds.

**Overclaiming readiness** → Mitigate with explicit evidence classes: harness-local, in-process-local, schedule-only gap, and snapshot-backed reproduced.

**API churn** → Mitigate by using existing SDK APIs in the first sample and adding helper APIs only after repeated boilerplate appears.

## Validation Plan

- Validate the OpenSpec strictly.
- Add local tests for default-disabled instrumentation and enabled in-process local output.
- Run the local template smoke and summary checker.
- Run optional VM accepted proof only after artifacts are prepared; keep it separate from fast harness acceptance.
