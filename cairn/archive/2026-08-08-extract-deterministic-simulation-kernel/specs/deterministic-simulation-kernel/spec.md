# Deterministic Simulation Kernel Specification

## Purpose

Run ChaosControl deterministic simulations from a pure Rust-owned core crate that contains every scheduling, network, fault, clock, and trace decision, while all KVM and machine effects stay behind a narrow shell execution boundary.

## ADDED Requirements

### Requirement: Simulation core contains only pure deterministic logic

r[chaoscontrol.simulation_kernel.pure_core] The `chaoscontrol-sim-core` crate SHALL contain only pure deterministic logic over in-memory state: tick advancement, vCPU scheduling decisions, network fabric transitions, fault schedule selection, virtual clock, deterministic RNG streams, and the event/trace model. The core MUST NOT perform KVM ioctls, filesystem access, wall-clock reads, environment inspection, process execution, or receipt writing.

#### Scenario: Core builds and tests without KVM

- GIVEN a host without KVM device access
- WHEN the core crate builds and its test suite runs
- THEN every test SHALL pass without touching `/dev/kvm`, the filesystem, the wall clock, or the environment.

#### Scenario: Core purity is machine-checked

r[chaoscontrol.simulation_kernel.validation.purity]
- GIVEN the core crate source
- WHEN the purity check scans its dependencies and source for KVM, filesystem, clock, environment, or process usage
- THEN any violation SHALL fail the check with a diagnostic naming the offending import or call.

### Requirement: Machine effects stay behind the execution boundary

r[chaoscontrol.simulation_kernel.execution_boundary] All KVM ioctls, guest memory and register access, interrupt delivery, and device MMIO handling SHALL live in shell crates behind the executor contract. The core SHALL consume `ExitObservation` values and emit `ExecutionCommand` values; it SHALL NOT hold file descriptors, guest memory handles, or callbacks into shell code.

#### Scenario: KVM shell maps commands and observations

- GIVEN a running deterministic simulation under the KVM executor
- WHEN the core emits an execution command
- THEN the shell SHALL map it to the corresponding KVM or device effect and return the resulting exit observation
- AND no shell type SHALL appear in the core's public API.

#### Scenario: Command and observation fixtures are admitted and rejected

r[chaoscontrol.simulation_kernel.validation.fixtures]
- GIVEN positive fixtures for canonical boot, network delivery, fault injection, and snapshot command sequences
- WHEN core boundary validation runs
- THEN valid sequences SHALL be accepted
- AND unknown observation kinds, out-of-order commands, and malformed DTOs SHALL fail with a diagnostic naming the invalid field.

### Requirement: Determinism is proven by trace equivalence

r[chaoscontrol.simulation_kernel.determinism] For a fixed seed, run config, and guest artifact set, the extracted engine SHALL emit a canonical event trace identical to the pre-split engine. Core decisions SHALL depend only on explicit inputs.

#### Scenario: Seeded migration equivalence

- GIVEN a fixed seed, config, and guest artifact set exercised before the split
- WHEN the extracted engine runs the same inputs
- THEN its canonical event trace SHALL equal the recorded pre-split trace.

#### Scenario: Mutated input diverges detectably

- GIVEN a recorded canonical trace
- WHEN the seed, config, or guest artifact set is mutated
- THEN trace comparison SHALL report the divergence at the first differing event.

### Requirement: Snapshot state models relocate without semantic change

r[chaoscontrol.simulation_kernel.snapshot_model] Pure snapshot state types and their validation MAY relocate to the core, but this change SHALL NOT add, remove, or reinterpret snapshot fields. Completeness semantics remain owned by `complete-vm-snapshot-state`; capture and restore effects remain in the shell.

#### Scenario: Relocated models validate identically

- GIVEN the snapshot validation fixtures accepted before relocation
- WHEN the relocated core validators run
- THEN every previously accepted fixture SHALL be accepted and every previously rejected fixture SHALL be rejected with an equivalent diagnostic.

### Requirement: Public surfaces stay compatible during migration

r[chaoscontrol.simulation_kernel.compatibility] The migration SHALL NOT change CLI output, artifact JSON field names, verdict schemas, evidence DTOs, or Nickel contract boundaries.

#### Scenario: Existing evidence artifacts remain valid

- GIVEN committed dogfood verdicts, bug records, and receipts
- WHEN the migrated evidence gates run
- THEN those artifacts SHALL validate exactly as before the split.

### Requirement: Extraction evidence rail

r[chaoscontrol.simulation_kernel.validation] The change SHALL complete only with core unit tests, purity checks, seeded trace-equivalence tests, negative divergence fixtures, workspace regression suites, and KVM smoke gates all green.

#### Scenario: Full gate set runs before archive

- GIVEN the extraction is implemented
- WHEN sync or archive is requested
- THEN the workspace test suite, clippy with warnings denied, the purity check, equivalence tests, evidence contract checks, and Cairn validation SHALL have passed on the final tree.
