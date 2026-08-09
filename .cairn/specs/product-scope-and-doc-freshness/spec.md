# Product Scope And Doc Freshness Specification

## Purpose

Defines the `product-scope-and-doc-freshness` capability.

## Requirements

### Requirement: Product scope uses a typed registry

r[chaoscontrol.product_scope.registry] ChaosControl MUST keep a typed Nickel registry for supported, experimental, deferred, blocked, and non-goal capabilities. Each entry MUST name its owner, boundary, evidence prerequisite, and documentation targets.

#### Scenario: Registry entry is complete
- GIVEN a capability has one valid state and all required ownership facts
- WHEN registry validation runs
- THEN it MUST produce one deterministic projection.

#### Scenario: Registry entry is ambiguous
- GIVEN a capability has conflicting states or lacks a required prerequisite
- WHEN registry validation runs
- THEN validation MUST fail.

### Requirement: Evidence controls support promotion

r[chaoscontrol.product_scope.promotion] A capability MUST NOT become supported until its named evidence prerequisite passes for the current admitted cohort.

#### Scenario: Implementation exists without evidence
- GIVEN product code exists but required evidence is missing or stale
- WHEN scope promotion runs
- THEN the capability MUST remain experimental, deferred, or blocked.

### Requirement: Active changes declare scope intent

r[chaoscontrol.product_scope.change_admission] Each active product or architecture change MUST name its target scope state, evidence prerequisite, owner, and non-claims.

#### Scenario: Experimental change enters validation
- GIVEN a change names an experimental state and preserves current non-goals
- WHEN lifecycle validation runs
- THEN the change MAY remain active without implying support.

#### Scenario: Change implies unsupported scope
- GIVEN a change implies hosted, cross-machine, container, or non-Rust support without an admitted scope decision
- WHEN lifecycle validation runs
- THEN validation MUST report a scope issue.

### Requirement: Factual documentation is generated from authority

r[chaoscontrol.product_scope.documentation] Workspace, test inventory, proof status, support state, and current architecture facts MUST come from named authoritative inputs rather than copied estimates.

#### Scenario: Repository facts change
- GIVEN an authoritative input differs from a marked document section
- WHEN freshness validation runs
- THEN validation MUST fail with the stale section and source.

#### Scenario: Historical document remains preserved
- GIVEN a document is explicitly marked historical
- WHEN freshness validation runs
- THEN it MAY preserve old facts without presenting them as current.

### Requirement: Scope decisions have a functional core

r[chaoscontrol.product_scope.functional_core] Scope-state, promotion, drift, and document-projection decisions MUST be pure deterministic logic. File reads, Cargo commands, and document writes MUST remain in shells.

#### Scenario: Identical facts are evaluated twice
- GIVEN identical registry, repository, evidence, and document facts
- WHEN the core evaluates them twice
- THEN both results MUST be identical.

### Requirement: Scope claims remain narrow

r[chaoscontrol.product_scope.boundary] Generated facts and support labels MUST NOT claim code quality, correctness, release eligibility, hosted service support, or universal determinism unless a separate accepted requirement admits that claim.

#### Scenario: Test count becomes a quality claim
- GIVEN a generated test count is presented as proof of correctness
- WHEN claim validation runs
- THEN validation MUST reject the promotion.

### Requirement: Scope and document validation is adversarial

r[chaoscontrol.product_scope.validation] Validation MUST include positive current projections and negative stale, conflicting, unsupported, blocked, missing-prerequisite, historical-mislabel, and overclaim cases.

#### Scenario: Closeout validation runs
- GIVEN maintainers intend to archive the change
- WHEN scope, document, lifecycle, and CI validation runs
- THEN every positive and negative class MUST produce its expected result.
