## ADDED Requirements

### Requirement: Workspace packages have an authoritative license map [r[license-boundary.package-map]]
ChaosControl SHALL maintain a complete package/path map that classifies repository-owned guest embedding surfaces as Apache-2.0 and host controller surfaces as AGPL-3.0-or-later.

#### Scenario: Every workspace package is classified [r[license-boundary.package-map.complete]]
- **GIVEN** the current Cargo workspace member list
- **WHEN** license-boundary validation runs
- **THEN** every repository-owned package is present exactly once in the authoritative map
- **AND** unknown or duplicate package entries fail validation

### Requirement: Package metadata follows runtime authority [r[license-boundary.package-metadata]]
Guest protocols, SDKs, guest support crates, guest fixtures, and copied workload templates SHALL remain Apache-2.0, while host fault, VMM, trace, exploration, dashboard, replay, and evidence crates SHALL declare AGPL-3.0-or-later.

#### Scenario: A workload links the guest SDK [r[license-boundary.package-metadata.guest]]
- **GIVEN** a downstream workload depends on `chaoscontrol-sdk` and `chaoscontrol-protocol`
- **WHEN** Cargo metadata is inspected
- **THEN** those packages report Apache-2.0
- **AND** they do not depend on a ChaosControl-owned AGPL host crate

#### Scenario: A controller package is distributed [r[license-boundary.package-metadata.host]]
- **GIVEN** a distributor packages the VMM, explorer, dashboard, replay, trace, fault, or evidence crate
- **WHEN** Cargo metadata is inspected
- **THEN** the package reports AGPL-3.0-or-later

### Requirement: Complete license artifacts accompany source [r[license-boundary.license-artifacts]]
ChaosControl SHALL ship complete Apache-2.0 and AGPL-3.0-or-later texts and SHALL provide a package/path boundary document.

#### Scenario: A mixed source archive is reviewed offline [r[license-boundary.license-artifacts.offline]]
- **GIVEN** an archive contains guest and host source
- **WHEN** a recipient inspects licensing without network access
- **THEN** both complete license texts and an unambiguous package/path map are available

### Requirement: Dependency policy recognizes the split [r[license-boundary.dependency-policy]]
The maintained dependency-license policy SHALL accept repository-owned AGPL host packages without weakening its existing allowlist, source provenance rules, or explicit exceptions for third-party crates.

#### Scenario: Cargo-deny evaluates the workspace [r[license-boundary.dependency-policy.cargo-deny]]
- **GIVEN** the intended mixed-license workspace
- **WHEN** cargo-deny license checks run
- **THEN** Apache and AGPL repository-owned packages pass
- **AND** a newly introduced unapproved third-party license still fails

### Requirement: Positive and negative fixtures guard mapping [r[license-boundary.fixtures]]
The license-boundary rail SHALL include positive fixtures for the intended package map and dependency direction and negative fixtures for reversed licenses, missing packages, and Apache guest crates depending on AGPL host crates.

#### Scenario: Intended mapping passes [r[license-boundary.positive-fixtures]]
- **GIVEN** every current package has the selected expression and dependency direction
- **WHEN** the fixture rail runs
- **THEN** validation passes deterministically

#### Scenario: Invalid mapping fails closed [r[license-boundary.negative-fixtures]]
- **GIVEN** a representative SDK/controller expression is reversed, a package is omitted, or an Apache guest crate depends on an AGPL host crate
- **WHEN** the fixture rail runs
- **THEN** validation fails with a package- or edge-specific diagnostic

### Requirement: License claims remain bounded [r[license-boundary.documentation]]
Current documentation SHALL state that processing a workload does not automatically license unrelated workload output, that package metadata does not rewrite existing evidence identity, that prior grants remain valid, and that third-party terms remain intact.

#### Scenario: A replay receipt is produced [r[license-boundary.documentation.output]]
- **GIVEN** an Apache-licensed workload is executed by an AGPL host controller
- **WHEN** ChaosControl emits a replay or evidence receipt
- **THEN** documentation does not claim the controller license automatically changes the workload or receipt owner's unrelated source license

### Requirement: Focused validation proves distribution consistency [r[license-boundary.final-validation]]
The change SHALL run strict OpenSpec validation and focused package metadata, dependency-policy, license-artifact, positive-fixture, and negative-fixture checks.

#### Scenario: Final validation runs [r[license-boundary.final-validation.complete]]
- **GIVEN** manifests, artifacts, policy, fixtures, and documentation agree
- **WHEN** the focused verification rail runs
- **THEN** it passes without making global VMM determinism or legal-compliance claims
