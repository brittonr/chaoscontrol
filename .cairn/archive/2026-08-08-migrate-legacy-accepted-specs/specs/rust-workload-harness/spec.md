# rust-workload-harness Specification

## Purpose

This specification defines ChaosControl's Rust-only workload harness adoption surface: local instrumentation feedback, downstream guest packaging, bounded workload run commands, and evidence classification boundaries for cross-project Rust use.
## Requirements
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
The harness report MUST guide Rust project instrumentation quality by summarizing assertion density categories when available, uncategorized assertions, never-reached assertions, sometimes assertions with no observed success, lifecycle readiness, instrumentation adoption track, and links to replay verdict/evidence artifacts when present.

#### Scenario: Report recommends in-process escalation [r[rust-workload-harness.instrumentation-quality.in-process-escalation]]
- GIVEN a workload has shallow external-harness coverage and documented invariants that are not visible through public APIs
- WHEN the instrumentation quality report or guide recommends next work
- THEN it can recommend moving selected assertions, lifecycle hooks, or guided randomness into service internals behind explicit gates
- AND it preserves the distinction between deeper local coverage and snapshot-backed replay evidence

### Requirement: Rust workload snapshot replay proof [r[rust-workload-harness.snapshot-replay-proof]]

ChaosControl MUST provide an opt-in Rust workload snapshot replay proof rail that converts the downstream-shaped Rust workload from bounded VM campaign evidence into accepted snapshot-backed replay evidence only when a persisted replay parent snapshot, exported bug, and reproduced replay verdict are all present.

#### Scenario: Probe is opt-in [r[rust-workload-harness.snapshot-replay-proof.opt-in]]

- GIVEN the Rust workload guest is run without the snapshot probe cmdline flag
- WHEN the workload executes local or bounded VM campaign behavior
- THEN the snapshot replay probe does not intentionally fail assertions

#### Scenario: Accepted replay verdict is required [r[rust-workload-harness.snapshot-replay-proof.accepted-verdict]]

- GIVEN the Rust workload snapshot probe is enabled and a parent-context bug is exported
- WHEN the replay proof rail runs standalone reproduce with `--verdict-output`
- THEN the proof is accepted only if the verdict has `replay_class = snapshot_backed_reproduced`, `reproduced = true`, `replay_parent_depth > 0`, and a valid digest-verified snapshot artifact

#### Scenario: Coverage manifest includes Rust workload [r[rust-workload-harness.snapshot-replay-proof.coverage-manifest]]

- GIVEN accepted Rust workload replay proof evidence exists
- WHEN replay proof coverage and readiness reports are generated
- THEN the Rust workload appears as a distinct accepted workload proof without weakening the existing Raft, redb, and net proof requirements

### Requirement: Rust-only SDK scope [r[rust-workload-harness.rust-only-scope]]

The Rust workload harness MUST treat Rust as the only supported SDK language for current product readiness and MUST NOT classify missing Go, Java, Python, C, or other SDKs as blockers for the current local ChaosControl product surface.

#### Scenario: Rust-only docs avoid language-gap framing [r[rust-workload-harness.rust-only-scope.docs]]

- GIVEN the Rust workload harness guide, template, or generated readiness summary is rendered
- WHEN it describes supported SDK scope
- THEN it states that Rust is the supported SDK surface for now
- AND it does not list non-Rust SDKs as active missing features or promotion blockers

### Requirement: Shared Rust simulator and VM adapter [r[rust-workload-harness.sim-vm-adapter]]

ChaosControl MUST provide a Rust workload adapter shape that can identify and configure the same workload for local in-process simulator runs and VM/hypervisor campaigns while preserving distinct evidence classes.

#### Scenario: Adapter identifies workload across modes [r[rust-workload-harness.sim-vm-adapter.identity]]

- GIVEN a Rust workload implements the shared adapter surface
- WHEN it is run in simulator mode and VM campaign mode
- THEN both receipts record the workload name, adapter version, scenario identity, selected seed or fault schedule reference, and relevant artifact digests
- AND the receipts label simulator-local and VM replay evidence separately

#### Scenario: Adapter rejects unsupported environment hooks [r[rust-workload-harness.sim-vm-adapter.unsupported-hooks]]

- GIVEN a workload adapter uses wall-clock time, host randomness, filesystem/network IO, or VM-only hypercalls without declaring the environment-specific hook
- WHEN the local simulator adapter validation runs
- THEN it rejects the adapter as unsupported simulator evidence without blocking VM-only campaign use

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
