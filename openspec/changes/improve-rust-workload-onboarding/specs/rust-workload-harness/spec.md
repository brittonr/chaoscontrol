## ADDED Requirements

### Requirement: Rust workload scaffold and promotion flow [r[rust-workload-harness.scaffold-promotion]]

ChaosControl MUST provide a Rust-only scaffold or template flow that creates a copyable workload harness, local dry-run command, bounded VM campaign command, and promotion checklist without requiring non-Rust SDKs, Docker/OCI, hosted setup, or vendoring ChaosControl internals.

#### Scenario: Scaffold produces runnable local dry-run [r[rust-workload-harness.scaffold-promotion.local-dry-run]]

- GIVEN a downstream Rust crate or the committed Rust workload template
- WHEN the scaffold/template flow is invoked
- THEN it produces a harness entry point that initializes the SDK, emits lifecycle setup, registers at least one categorized assertion, and documents a local dry-run command that writes JSONL plus a structured report

#### Scenario: Promotion checklist preserves evidence classes [r[rust-workload-harness.scaffold-promotion.evidence-classes]]

- GIVEN a scaffolded workload has local report output, bounded VM campaign output, and optional exported bug artifacts
- WHEN the promotion checklist or checker runs
- THEN it classifies local instrumentation, schedule-only replay, and snapshot-backed replay proof separately
- AND it refuses to promote the workload as supported without accepted verdict evidence and snapshot artifact validation

### Requirement: Rust assertion quality gate [r[rust-workload-harness.assertion-quality-gate]]

The Rust workload harness MUST provide a deterministic assertion quality gate that evaluates local dry-run output before a VM campaign and reports weak instrumentation without claiming replay proof.

#### Scenario: Quality gate catches weak local reports [r[rust-workload-harness.assertion-quality-gate.weak-report]]

- GIVEN a local SDK report with missing setup lifecycle, uncategorized assertions, unobserved reachability assertions, or sometimes assertions with no success
- WHEN the assertion quality gate runs
- THEN it exits nonzero or records explicit blockers with assertion IDs/messages and recommended Rust-side fixes

#### Scenario: Quality gate accepts credible local report [r[rust-workload-harness.assertion-quality-gate.accepts-report]]

- GIVEN a local SDK report with setup lifecycle, categorized assertions, observed core scenarios, and no failing ordinary assertions
- WHEN the assertion quality gate runs
- THEN it emits a stable summary suitable for CI while stating that local quality is not snapshot replay evidence
