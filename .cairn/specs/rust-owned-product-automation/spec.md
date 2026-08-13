# Rust Owned Product Automation Specification

## Purpose

Defines the `rust-owned-product-automation` capability.

## Requirements

### Requirement: Python behavior is inventoried before migration

r[chaoscontrol.rust_automation.inventory] Each Python script and inline Python block MUST have a recorded owner, inputs, outputs, exit classes, bounds, effects, callers, and positive and negative fixtures before replacement.

#### Scenario: Surface is fully inventoried
- GIVEN one Python surface has every required fact and fixture
- WHEN migration admission runs
- THEN the surface MAY enter Rust replacement.

#### Scenario: Hidden caller remains
- GIVEN a Python surface has an unrecorded Nix or script caller
- WHEN removal admission runs
- THEN removal MUST remain blocked.

### Requirement: Evidence automation has Rust ownership

r[chaoscontrol.rust_automation.evidence] Accepted snapshot dogfood, receipt materialization, and dogfood summary decisions MUST use evidence-owned Rust DTOs and pure classification cores.

#### Scenario: Dogfood output is valid
- GIVEN a complete bounded dogfood output directory
- WHEN the Rust core classifies it
- THEN it MUST produce the compatibility summary and receipt plan.

#### Scenario: Dogfood output is malformed
- GIVEN a required artifact or field is missing or stale
- WHEN classification runs
- THEN it MUST return the expected typed failure without partial promotion.

### Requirement: KVM smoke orchestration is typed

r[chaoscontrol.rust_automation.kvm] Local multi-hypervisor KVM smoke orchestration MUST use typed plans, bounded process outcomes, and Rust-owned receipt models.

#### Scenario: One hypervisor row times out
- GIVEN the runner reaches a row timeout
- WHEN the campaign summary is built
- THEN the row MUST be incomplete or failed and MUST NOT count as passed.

### Requirement: Repository tools have focused Rust owners

r[chaoscontrol.rust_automation.tools] Cargo audit policy and workload scaffold transformation MUST use separate focused Rust owners with deterministic validation cores.

#### Scenario: Audit report has an unallowed advisory
- GIVEN the parsed report contains a finding outside the admitted allowlist
- WHEN policy evaluation runs
- THEN the tool MUST return a failing typed result.

### Requirement: Nix invokes compiled product tools

r[chaoscontrol.rust_automation.nix] Nix apps and checks MUST invoke compiled Rust tools for structured parsing, validation, classification, and rendering. Shell glue MUST NOT own those decisions.

#### Scenario: Nix wrapper runs a migrated command
- GIVEN the compiled binary is present
- WHEN the app runs
- THEN the wrapper MUST pass explicit paths and arguments without inline Python.

### Requirement: Cutover preserves compatibility

r[chaoscontrol.rust_automation.parity] Migration MUST preserve public command names, machine-readable schemas, exit classes, artifact plans, and bounded semantics unless a separate change admits a break.

#### Scenario: Frozen fixture is compared
- GIVEN old and new implementations read one frozen fixture
- WHEN parity validation runs
- THEN canonical output and exit class MUST match.

### Requirement: Removal follows caller migration

r[chaoscontrol.rust_automation.removal] Python scripts, inline blocks, and runtime inputs MUST be removed only after all admitted callers use Rust and parity validation passes.

#### Scenario: One Nix app still requires Python
- GIVEN a product app retains a Python runtime input
- WHEN removal admission runs
- THEN the migration MUST remain incomplete.

### Requirement: Automation decisions have a functional core

r[chaoscontrol.rust_automation.functional_core] Parsing into typed DTOs, validation, classification, summary models, manifest plans, and audit policy MUST be pure. Files, processes, directories, writes, and output MUST remain in shells.

#### Scenario: Identical inputs are evaluated twice
- GIVEN identical loaded DTOs
- WHEN a core evaluates them twice
- THEN both results MUST be identical.

### Requirement: Automation claims remain bounded

r[chaoscontrol.rust_automation.boundary] Rust ownership MUST NOT claim orchestration correctness, complete audits, KVM behavior, sandboxing, or release eligibility.

#### Scenario: Migration is presented as correctness proof
- GIVEN Python has been removed
- WHEN a report claims the workflows are correct because they use Rust
- THEN claim validation MUST reject the report.

### Requirement: Rust automation validation is adversarial

r[chaoscontrol.rust_automation.validation] Validation MUST pair positive fixtures with malformed JSON, missing fields, stale artifacts, timeouts, partial output, unsafe paths, permission failures, write failures, and caller-drift cases.

#### Scenario: Closeout validation runs
- GIVEN maintainers intend to remove Python
- WHEN parity, focused, Nix, workspace, and lifecycle validation runs
- THEN every caller MUST use Rust and every negative fixture MUST fail as specified.
