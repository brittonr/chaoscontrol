# Dogfood Artifact Retention Specification

## Purpose

Keep reviewable evidence in Git while storing large payloads through exact content-addressed references.

## ADDED Requirements

### Requirement: Large artifacts use storage-neutral object references

r[chaoscontrol.dogfood_artifacts.object_ref] Each externalized object MUST bind a domain, BLAKE3 digest, exact byte length, media type, and evidence role. Storage location MUST NOT change object identity.

#### Scenario: Object reference is complete
- GIVEN every identity field is valid
- WHEN reference validation runs
- THEN it MUST produce one deterministic object identity.

#### Scenario: Object reference is ambiguous
- GIVEN the algorithm, digest, size, media type, or role is missing
- WHEN reference validation runs
- THEN validation MUST fail.

### Requirement: Live references are inventoried

r[chaoscontrol.dogfood_artifacts.inventory] Migration MUST identify every tracked large payload, live manifest reference, duplicate cohort, diagnostic-only artifact, and raw debug output before deletion.

#### Scenario: Unclassified tracked payload exists
- GIVEN a tracked large payload has no live, diagnostic, duplicate, or expired class
- WHEN migration admission runs
- THEN deletion MUST remain blocked.

### Requirement: Materialization validates exact bytes

r[chaoscontrol.dogfood_artifacts.materialization] A materialized object MUST match its exact byte length, BLAKE3, media type, role, and manifest linkage before replay or evidence use.

#### Scenario: Object materializes exactly
- GIVEN a candidate object matches every reference fact
- WHEN materialization validation runs
- THEN the consumer MAY use its staged path.

#### Scenario: Object is missing or corrupt
- GIVEN a candidate is absent, truncated, oversized, or has the wrong digest
- WHEN materialization validation runs
- THEN the operation MUST fail before replay or promotion.

### Requirement: Retention uses typed policy

r[chaoscontrol.dogfood_artifacts.retention] A typed Nickel policy MUST define retained cohort classes, diagnostic exemplars, expiration, tracked-size limits, duplicate rules, raw-log rules, and adapter bounds.

#### Scenario: Raw debug log enters Git
- GIVEN a raw run or reproduction log lacks an admitted summary role
- WHEN retention validation runs
- THEN validation MUST reject the tracked artifact.

### Requirement: Migration is two-phase

r[chaoscontrol.dogfood_artifacts.migration] Tracked large payloads MUST remain until live object references materialize and all linked replay and readiness gates pass. Removal MUST occur in a separate reviewed phase.

#### Scenario: One live object cannot materialize
- GIVEN any live manifest references an unavailable object
- WHEN deletion admission runs
- THEN removal of its tracked source MUST remain blocked.

### Requirement: Artifact decisions have a functional core

r[chaoscontrol.dogfood_artifacts.functional_core] Reference, linkage, size, digest, role, retention, duplicate, and deletion decisions MUST be pure deterministic logic. Fetch, staging, files, and publication MUST remain in shells.

#### Scenario: Identical inventory is evaluated twice
- GIVEN identical references, policy, and observations
- WHEN the core evaluates them twice
- THEN both plans MUST be identical.

### Requirement: Artifact claims remain bounded

r[chaoscontrol.dogfood_artifacts.boundary] Successful materialization MUST NOT prove storage durability, availability, authorization, trust, replay success, or release eligibility.

#### Scenario: Store success becomes replay proof
- GIVEN an object materializes correctly
- WHEN a report claims replay success without execution
- THEN claim validation MUST reject the report.

### Requirement: Artifact retention validation is adversarial

r[chaoscontrol.dogfood_artifacts.validation] Validation MUST pair exact materialization with missing, corrupt, truncated, wrong-size, wrong-role, unsafe-path, duplicate, unavailable, and deletion-blocked cases.

#### Scenario: Closeout validation runs
- GIVEN maintainers intend to remove tracked payloads
- WHEN inventory, materialization, replay, readiness, and repository gates run
- THEN every live reference MUST pass and every negative fixture MUST fail as specified.
