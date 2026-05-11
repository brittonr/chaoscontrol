# vm-determinism-drift Specification

## Purpose
Defines the bounded ChaosControl VM determinism drift gate: repeated same-input VM/controller runs, machine-readable receipt output, pure fingerprint comparison, and optional dlog structural evidence without claiming universal guest/device determinism.
## Requirements
### Requirement: VM determinism drift receipt [r[vm-determinism-drift.receipt]]
The system MUST emit a machine-readable receipt for an operator-invoked VM determinism drift gate.

#### Scenario: Receipt binds run inputs [r[vm-determinism-drift.receipt.inputs]]
- **GIVEN** an operator runs the determinism stress gate with a kernel path, initrd path, and receipt path
- **WHEN** the gate finishes
- **THEN** the receipt records the gate name, schema version, kernel path, initrd path, and deterministic input fingerprints
- **AND** the receipt records aggregate pass/fail status

#### Scenario: Receipt records per-case fingerprints [r[vm-determinism-drift.receipt.cases]]
- **GIVEN** the gate executes repeated same-seed VM or controller configurations
- **WHEN** each configuration completes
- **THEN** the receipt records the reference fingerprint, every observed run fingerprint, and any mismatch details

### Requirement: Pure drift comparison core [r[vm-determinism-drift.comparison]]
The system MUST keep VM drift comparison and receipt aggregation available as Rust-owned pure logic that can be tested without KVM.

#### Scenario: Identical observations pass [r[vm-determinism-drift.comparison.identical]]
- **GIVEN** two observations with identical VM fingerprints
- **WHEN** the pure comparison core builds a case report
- **THEN** the report is marked passed and contains no mismatch details

#### Scenario: Changed observations fail with a field class [r[vm-determinism-drift.comparison.mismatch]]
- **GIVEN** a later observation differs from the reference observation
- **WHEN** the pure comparison core builds a case report
- **THEN** the report is marked failed and identifies the mismatched field

### Requirement: Bounded operator profile [r[vm-determinism-drift.operator-profile]]
The system MUST expose a named bounded operator profile for the VM determinism drift gate so current DST confidence can be rerun without rediscovering kernel/initrd/profile arguments.

#### Scenario: Hide-TSC profile is the operator default [r[vm-determinism-drift.operator-profile.hide-tsc-default]]
- **GIVEN** an operator invokes the packaged VM determinism drift gate without low-level clock-profile arguments
- **WHEN** the gate launches the repeated VM/controller stress runner
- **THEN** it uses the hide-TSC clock profile, emits a receipt, and captures dlogs under the requested output directory
- **AND** the operator-facing documentation states that this is a bounded profile-specific drift check rather than universal guest/device/timing determinism proof

#### Scenario: Legacy TSC remains available for A/B diagnosis [r[vm-determinism-drift.operator-profile.legacy-tsc]]
- **GIVEN** an engineer needs to compare the current operator profile against the legacy TSC baseline
- **WHEN** they invoke the low-level stress runner with an explicit TSC clock profile
- **THEN** the runner accepts the explicit profile and does not silently hide the TSC feature

### Requirement: Optional dlog structural evidence [r[vm-determinism-drift.dlog]]
The system MUST allow the drift gate to compare optional dlog structural traces without requiring dlogs in every low-level stress path.

#### Scenario: Dlog directory is opt-in [r[vm-determinism-drift.dlog.opt-in]]
- **GIVEN** an operator runs the drift gate without a dlog directory
- **WHEN** the gate emits a receipt
- **THEN** the dlog structural match field may be absent for each case

#### Scenario: Structural dlog mismatch fails the case [r[vm-determinism-drift.dlog.mismatch]]
- **GIVEN** an operator runs the drift gate with dlog capture enabled
- **WHEN** any non-reference dlog differs structurally from the reference dlog
- **THEN** the affected case is marked failed even if the coarse fingerprint fields match

### Requirement: Device/profile determinism matrix [r[vm-determinism-drift.device-profile-matrix]]
The system MUST provide a bounded device/profile determinism matrix that records selected guest, kernel, device, clock, and controller profiles without promoting those observations into arbitrary determinism proof.

#### Scenario: Matrix receipt binds profile rows [r[vm-determinism-drift.device-profile-matrix.rows]]
- **GIVEN** an operator runs a bounded determinism matrix over selected profiles
- **WHEN** the matrix receipt is emitted
- **THEN** each row records the guest/workload identity, kernel or initrd fingerprint, device profile, clock profile, controller configuration, observed fingerprints, and pass/fail status
- **AND** the receipt states that unlisted profiles remain unproven

#### Scenario: Matrix validation fails on missing or duplicate profile rows [r[vm-determinism-drift.device-profile-matrix.fail-closed]]
- **GIVEN** a matrix receipt with missing required rows, duplicate row identifiers, or weakened bounded-scope language
- **WHEN** the matrix validator runs
- **THEN** it MUST reject the receipt before any readiness surface can cite it as promotion evidence

### Requirement: Determinism matrix negative evidence [r[vm-determinism-drift.matrix-negative-evidence]]
The system MUST include negative matrix evidence that proves drift, stale rows, and anti-claim weakening are detected by deterministic checks.

#### Scenario: Mismatched observations fail the matrix [r[vm-determinism-drift.matrix-negative-evidence.mismatch]]
- **GIVEN** a matrix fixture where a non-reference observation differs from the row reference
- **WHEN** the pure matrix comparison runs
- **THEN** it MUST mark the row failed and identify the mismatched field class

#### Scenario: Universal determinism wording fails promotion [r[vm-determinism-drift.matrix-negative-evidence.overclaim]]
- **GIVEN** a readiness report that describes bounded matrix evidence as arbitrary guest/device determinism proof
- **WHEN** the promotion gate runs
- **THEN** it MUST exit nonzero and identify the overclaimed determinism surface

