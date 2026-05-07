## MODIFIED Requirements

### Requirement: Local dry-run instrumentation report [r[rust-workload-harness.local-report]]
The harness MUST provide a local dry-run mode that runs outside a ChaosControl VM and emits a structured instrumentation report. The report MUST include the assertion catalog, lifecycle events observed, reached and unreached assertion sites when knowable from the local run, sometimes assertion observations, random-choice call-site observations or counts, and per-assertion registered-vs-observed coverage details.

#### Scenario: Dry-run catches missing setup signal [r[rust-workload-harness.local-report.setup-missing]]
- GIVEN a harness workload that never emits setup-complete
- WHEN the workload is executed in local dry-run mode
- THEN the report flags the missing setup-complete lifecycle event before any VM campaign is required

#### Scenario: Dry-run shows assertion exercise [r[rust-workload-harness.local-report.assertions]]
- GIVEN a harness workload with registered assertions
- WHEN the workload is executed in local dry-run mode
- THEN the report separates cataloged assertions from assertions exercised by that run
- AND the report identifies sometimes/reachable assertions that did not make progress in the dry-run

#### Scenario: Dry-run lists unobserved registered assertions [r[rust-workload-harness.local-report.unobserved-details]]
- GIVEN a harness workload with registered assertion catalog entries that are not hit in a local dry-run
- WHEN the local report is generated
- THEN the report includes deterministic per-assertion entries with ID, message, type, category, observed hit count, success/failure counts, and observed/unobserved status

### Requirement: Rust workload harness surface [r[rust-workload-harness.surface]]
ChaosControl MUST provide a Rust-only workload harness or template layer that downstream Rust projects can use without copying ChaosControl guest boilerplate. The harness MUST initialize the SDK, expose a setup phase, expose one or more scenario entry points, preserve access to existing SDK assertion, lifecycle, guidance, and random APIs, and provide a copyable golden-path template using only public SDK APIs.

#### Scenario: Downstream Rust project wires harness [r[rust-workload-harness.surface.downstream]]
- GIVEN a downstream Rust project that depends on `chaoscontrol-sdk` and the harness surface
- WHEN the project defines setup and scenario functions using the harness
- THEN the project can call SDK assertions, lifecycle events, and guided randomness without writing ChaosControl VM transport glue

#### Scenario: Existing SDK APIs remain usable [r[rust-workload-harness.surface.sdk-compat]]
- GIVEN a workload using the harness
- WHEN code calls existing `chaoscontrol_sdk::assert`, `lifecycle`, `guidance`, or `random` APIs directly
- THEN those calls continue to compile and emit through the same SDK transport/local-output behavior as non-harness code

#### Scenario: Golden-path template starts from local dry-run [r[rust-workload-harness.surface.template-local-first]]
- GIVEN a downstream Rust user evaluating ChaosControl for the first time
- WHEN they copy the documented Rust workload template
- THEN the template points them at a local dry-run and assertion coverage report before any VM or replay proof rail
