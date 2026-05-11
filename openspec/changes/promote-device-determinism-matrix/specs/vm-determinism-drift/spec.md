## ADDED Requirements

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
