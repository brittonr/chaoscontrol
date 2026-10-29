# Raw Registration Identity Specification

## Purpose

Defines the `raw-registration-identity` capability.

## Requirements

### Requirement: Raw registration reuse preserves exact property identity

r[chaoscontrol.raw_registration.exact_reuse]

An existing raw alias MUST be reusable only by the same complete descriptor and source/display/must-hit registration metadata. Distinct colliding string IDs and changed metadata MUST be rejected without altering accepted catalog identity, observation counts or sparse-sampling state. Exact duplicate registrations and legitimate positive/negative observations MUST remain usable, and unknown late sites MUST remain rejected.

#### Scenario: Colliding ID cannot emit as another property

- GIVEN a declared raw property and a different raw string ID with the same compatibility alias, kind and message
- WHEN the conflicting ID attempts declaration or observation
- THEN it is rejected and no hit is attributed to the original property

#### Scenario: Registration metadata is immutable

- GIVEN an accepted raw registration
- WHEN a subsequent call changes descriptor, source, display or must-hit metadata
- THEN the call is rejected without consuming the original site's sparse outcome sample

#### Scenario: Genuine outcomes survive rejected attempts

- GIVEN rejected conflicting calls and an exact original registration
- WHEN legitimate passing and failing observations are emitted
- THEN original IDs, fingerprints, source fields and actual conditions are retained, with default per-hit or sparse first-outcome counts as configured
