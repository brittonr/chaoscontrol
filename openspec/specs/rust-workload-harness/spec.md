# rust-workload-harness Specification

## Purpose

This specification defines ChaosControl's Rust-only workload harness adoption surface: local instrumentation feedback, downstream guest packaging, bounded workload run commands, and evidence classification boundaries for cross-project Rust use.
## Requirements
### Requirement: Rust workload harness surface [r[rust-workload-harness.surface]]

ChaosControl MUST provide a Rust-only workload harness or template layer that downstream Rust projects can use without copying ChaosControl guest boilerplate. The harness MUST initialize the SDK, expose a setup phase, expose one or more scenario entry points, and preserve access to existing SDK assertion, lifecycle, guidance, and random APIs.

#### Scenario: Downstream Rust project wires harness [r[rust-workload-harness.surface.downstream]]

- GIVEN a downstream Rust project that depends on `chaoscontrol-sdk` and the harness surface
- WHEN the project defines setup and scenario functions using the harness
- THEN the project can call SDK assertions, lifecycle events, and guided randomness without writing ChaosControl VM transport glue

#### Scenario: Existing SDK APIs remain usable [r[rust-workload-harness.surface.sdk-compat]]

- GIVEN a workload using the harness
- WHEN code calls existing `chaoscontrol_sdk::assert`, `lifecycle`, `guidance`, or `random` APIs directly
- THEN those calls continue to compile and emit through the same SDK transport/local-output behavior as non-harness code

### Requirement: Local dry-run instrumentation report [r[rust-workload-harness.local-report]]

The harness MUST provide a local dry-run mode that runs outside a ChaosControl VM and emits a structured instrumentation report. The report MUST include the assertion catalog, lifecycle events observed, reached and unreached assertion sites when knowable from the local run, sometimes assertion observations, and random-choice call-site observations or counts.

#### Scenario: Dry-run catches missing setup signal [r[rust-workload-harness.local-report.setup-missing]]

- GIVEN a harness workload that never emits setup-complete
- WHEN the workload is executed in local dry-run mode
- THEN the report flags the missing setup-complete lifecycle event before any VM campaign is required

#### Scenario: Dry-run shows assertion exercise [r[rust-workload-harness.local-report.assertions]]

- GIVEN a harness workload with registered assertions
- WHEN the workload is executed in local dry-run mode
- THEN the report separates cataloged assertions from assertions exercised by that run
- AND the report identifies sometimes/reachable assertions that did not make progress in the dry-run

### Requirement: Downstream guest packaging rail [r[rust-workload-harness.packaging]]

ChaosControl MUST provide a Nix and/or CLI rail that packages a downstream Rust workload binary as a ChaosControl guest without requiring the downstream project to clone, edit, or vendor ChaosControl internals. The rail MUST make the selected guest binary, kernel/initrd composition inputs, workload name, VM count, round bound, and extra kernel command line inspectable in the resulting command or derivation.

#### Scenario: Flake helper packages external guest [r[rust-workload-harness.packaging.flake]]

- GIVEN a downstream flake with a Rust workload package
- WHEN the project calls the ChaosControl workload packaging helper with that package
- THEN the helper produces a guest/initrd or runnable check suitable for `chaoscontrol-explore`

#### Scenario: Packaging rail exposes bounded defaults [r[rust-workload-harness.packaging.defaults]]

- GIVEN a downstream workload using default harness settings
- WHEN the packaging rail renders the campaign command or derivation
- THEN the VM count, round bound, workload name, and extra command line are visible and overrideable

### Requirement: One-command bounded workload run [r[rust-workload-harness.run-command]]

ChaosControl MUST provide a single documented command or flake app that runs a bounded campaign for a harness workload from a downstream Rust project. The command MUST build the guest, run local or VM execution as requested, and write a report path that can be inspected after completion.

#### Scenario: Run command executes sample workload [r[rust-workload-harness.run-command.sample]]

- GIVEN a sample Rust workload using the harness
- WHEN the user runs the documented bounded workload command
- THEN ChaosControl builds the guest, runs the bounded campaign, and writes a report path

#### Scenario: Run command preserves replay evidence boundary [r[rust-workload-harness.run-command.evidence-boundary]]

- GIVEN a bounded workload run that finds a bug
- WHEN the run writes report output
- THEN the report distinguishes a local/dry-run finding, a schedule-only reproduction gap, and an accepted snapshot-backed replay verdict rather than promoting all findings equally

#### Scenario: VM validation command completes [r[rust-workload-harness.run-command.vm-validation]]

- GIVEN the Rust workload harness VM rail and a machine capable of running the campaign
- WHEN `.#explore-rust-workload` is run with a writable output directory and sufficient build/runtime budget
- THEN the command completes and writes inspectable VM campaign output and an evidence classification receipt

### Requirement: Cross-project instrumentation quality report [r[rust-workload-harness.instrumentation-quality]]

The harness report MUST guide Rust project instrumentation quality by summarizing assertion density categories when available, uncategorized assertions, never-reached assertions, sometimes assertions with no observed success, lifecycle readiness, and links to replay verdict/evidence artifacts when present.

#### Scenario: Report recommends next instrumentation work [r[rust-workload-harness.instrumentation-quality.next-work]]

- GIVEN a workload run with uncategorized assertions and never-reached reachable assertions
- WHEN the report is generated
- THEN it includes those gaps in a concise instrumentation quality section suitable for deciding what to instrument next

#### Scenario: Report links VM evidence [r[rust-workload-harness.instrumentation-quality.evidence-links]]

- GIVEN a VM campaign run that produced exported bug and replay verdict artifacts
- WHEN the harness report is generated
- THEN it includes paths to the evidence directory, exported bug artifact, and replay verdict artifact
