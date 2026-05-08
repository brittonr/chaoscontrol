## MODIFIED Requirements

### Requirement: Rust workload harness surface [r[rust-workload-harness.surface]]
ChaosControl MUST provide a Rust-only workload harness or template layer that downstream Rust projects can use without copying ChaosControl guest boilerplate. The harness MUST initialize the SDK, expose a setup phase, expose one or more scenario entry points, preserve access to existing SDK assertion, lifecycle, guidance, and random APIs, and provide a copyable golden-path template using only public SDK APIs. The adoption surface MUST also document an advanced opt-in in-process instrumentation path for downstream Rust services that need SDK assertions, lifecycle hooks, or guided randomness inside service code rather than only in an external workload driver.

#### Scenario: Advanced in-process path is explicit [r[rust-workload-harness.surface.in-process-explicit]]
- GIVEN a downstream Rust service starts from the default external harness path
- WHEN important invariants are not observable through public service APIs or workload-driver behavior
- THEN the documentation presents in-process instrumentation as an advanced escalation path
- AND the path requires explicit feature, cfg, or runtime configuration gates before SDK calls are placed in service internals

#### Scenario: Default path remains non-invasive [r[rust-workload-harness.surface.default-non-invasive]]
- GIVEN a downstream Rust user follows the golden-path template
- WHEN they run the initial local dry-run
- THEN ChaosControl does not require service-internal SDK calls, production code changes, Docker/Kubernetes integration, or hosted product setup

### Requirement: Local dry-run instrumentation report [r[rust-workload-harness.local-report]]
The harness MUST provide a local dry-run mode that runs outside a ChaosControl VM and emits a structured instrumentation report. The report MUST include the assertion catalog, lifecycle events observed, reached and unreached assertion sites when knowable from the local run, sometimes assertion observations, random-choice call-site observations or counts, and per-assertion registered-vs-observed coverage details. When observations include service-internal SDK calls, the report MUST identify the adoption track or instrumentation source so operators can distinguish external-harness observations from in-process-service observations.

#### Scenario: Report distinguishes in-process observations [r[rust-workload-harness.local-report.in-process-source]]
- GIVEN a local run that drives a workload externally and also observes SDK assertions emitted from service-internal code
- WHEN the local instrumentation report is generated
- THEN the report separates or labels external-harness and in-process-service observations
- AND the report does not classify either local observation source as accepted replay proof without a replay verdict artifact

### Requirement: Cross-project instrumentation quality report [r[rust-workload-harness.instrumentation-quality]]
The harness report MUST guide Rust project instrumentation quality by summarizing assertion density categories when available, uncategorized assertions, never-reached assertions, sometimes assertions with no observed success, lifecycle readiness, instrumentation adoption track, and links to replay verdict/evidence artifacts when present.

#### Scenario: Report recommends in-process escalation [r[rust-workload-harness.instrumentation-quality.in-process-escalation]]
- GIVEN a workload has shallow external-harness coverage and documented invariants that are not visible through public APIs
- WHEN the instrumentation quality report or guide recommends next work
- THEN it can recommend moving selected assertions, lifecycle hooks, or guided randomness into service internals behind explicit gates
- AND it preserves the distinction between deeper local coverage and snapshot-backed replay evidence
