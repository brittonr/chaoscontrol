# Unified Agpl License Specification

## Purpose

Defines the `unified-agpl-license` capability.

## Requirements

### Requirement: Repository-owned source uses one license

r[chaoscontrol.unified_agpl.scope] Every future revision of repository-owned crates, tools, templates, fixtures, lifecycle material, and documentation MUST use `AGPL-3.0-or-later` unless an explicit path exception records another valid grant.

#### Scenario: A repository-owned package is published

- GIVEN a package contains only repository-owned source and compatible dependencies
- WHEN its source archive and package metadata are produced
- THEN the package MUST declare `AGPL-3.0-or-later`
- AND the archive MUST include the complete corresponding license text.

### Requirement: License changes require authority

r[chaoscontrol.unified_agpl.authority] The migration MUST classify ownership and license authority for every changed path. It MUST NOT replace third-party, upstream-derived, or unknown-authority terms without a compatible grant.

#### Scenario: A path has third-party terms

- GIVEN a source or generated path contains third-party material
- WHEN the license inventory evaluates that path
- THEN the original terms and notices MUST remain visible
- AND the repository policy MUST NOT describe that material as solely project-owned AGPL source.

### Requirement: Prior grants remain valid

r[chaoscontrol.unified_agpl.prior_grants] The repository MUST state that earlier Apache-2.0 releases and grants remain valid. The unified policy MUST NOT claim retroactive withdrawal of those rights.

#### Scenario: A consumer uses an earlier Apache release

- GIVEN a consumer obtained a published Apache-2.0 version before the unified boundary
- WHEN the current license documentation describes the migration
- THEN it MUST preserve the earlier grant
- AND it MUST identify the later boundary without claiming that the earlier grant ended.

### Requirement: Package artifacts match metadata

r[chaoscontrol.unified_agpl.metadata] Cargo metadata, crate-local license files, source archives, and repository license maps MUST agree for every repository-owned package.

#### Scenario: Package metadata remains stale

- GIVEN an authorized repository-owned package still declares Apache-2.0 or carries only an Apache license file
- WHEN package policy checks run
- THEN the package MUST fail the unified-license check
- AND it MUST NOT be published as compliant.

### Requirement: Distributed templates carry explicit terms

r[chaoscontrol.unified_agpl.templates] Repository-owned source templates and generated copies MUST carry visible `AGPL-3.0-or-later` terms. Generation guidance MUST describe those terms before distribution.

#### Scenario: A workload scaffold is generated

- GIVEN a repository-owned template produces Rust source for a downstream workload
- WHEN the scaffold is generated
- THEN the copied repository-owned source MUST include an AGPL notice
- AND the generator documentation MUST identify its license.

### Requirement: Runtime output is not source relicensing

r[chaoscontrol.unified_agpl.outputs] Running a workload through ChaosControl MUST NOT by itself change the license of unrelated workload source, VM output, reports, traces, receipts, or artifacts.

#### Scenario: An external workload emits a report

- GIVEN an external workload has independent terms
- WHEN an AGPL ChaosControl controller processes it and emits a report
- THEN the report MUST remain outside the repository source-license claim unless an explicit format grant says otherwise.

### Requirement: Dependency review remains strict

r[chaoscontrol.unified_agpl.dependency_policy] License policy MUST admit the repository-owned AGPL packages without weakening review for third-party dependencies or incompatible terms.

#### Scenario: An incompatible dependency enters the graph

- GIVEN a new third-party dependency has terms outside the accepted policy
- WHEN dependency policy checks run
- THEN the checks MUST fail with the package and license expression
- AND the unified project license MUST NOT suppress that failure.

### Requirement: License migration has positive and negative checks

r[chaoscontrol.unified_agpl.validation] The migration MUST check valid package archives and MUST reject stale metadata, missing texts, mismatched template notices, and accidental relabeling of excluded paths.

#### Scenario: Full license checks run

- GIVEN the path inventory, package archives, templates, and dependency graph
- WHEN the unified license checks run
- THEN every authorized project path MUST report consistent AGPL terms
- AND every excluded or malformed case MUST retain its terms or fail with a typed diagnostic.
