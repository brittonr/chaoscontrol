# Setup Json Details Specification

## Purpose

Defines the `setup-json-details` capability.

## Requirements

### Requirement: Setup details follow the SDK JSON value contract

r[chaoscontrol.setup_json_details.shapes]

Local setup evidence MUST accept every bounded JSON value as explicit `details`, including null, booleans, numbers, strings, arrays and objects, matching the existing Rust SDK lifecycle contract. Reports MUST retain completed setup state and existing assertion identity/counters. Adoption metadata MUST be extracted only from the top-level object, never scalar text or nested array members.

#### Scenario: A Rust workload supplies non-object setup details

- GIVEN valid accepted assertion catalog/hit records and a complete setup record with scalar, array or null details
- WHEN the local stream is summarized and the resulting report checked
- THEN setup completion is recorded, the assertion verdict/identity is unchanged, and no adoption track is inferred from nested or scalar data

### Requirement: JSON details do not weaken lifecycle evidence bounds

r[chaoscontrol.setup_json_details.bounds]

The `details` field MUST remain required even though its value may be null. Setup status, unknown-field and duplicate-setup checks, existing object metadata key/field limits, and generic JSON/stream resource limits MUST remain enforced. Object details MUST retain their existing adoption metadata behavior. Evidence remains local instrumentation, not VM or replay proof.

#### Scenario: Shape flexibility preserves admission failures

- GIVEN setup records with omitted details, invalid status, unknown fields, duplicate setup, over-bound object metadata or over-bound JSON structures
- WHEN local evidence is admitted
- THEN the invalid stream is rejected while an in-bound object at the metadata boundary remains accepted
