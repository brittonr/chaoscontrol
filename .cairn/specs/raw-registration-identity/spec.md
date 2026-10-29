# Raw Registration Identity Specification

## Purpose

Defines the `raw-registration-identity` capability.

## Requirements

### Requirement: Raw registration reuse preserves exact property identity

r[chaoscontrol.raw_registration.exact_reuse]

An existing raw logical key MUST be reusable only by the same complete descriptor and source/display/must-hit registration metadata. Changed metadata MUST be rejected without altering accepted catalog identity, observation counts or sparse-sampling state. Distinct string keys MUST remain independent even when their historical compact hashes collide. Exact duplicate registrations and legitimate positive/negative observations MUST remain usable, and unknown late keys MUST remain rejected.

#### Scenario: Same key cannot change identity

- GIVEN an accepted raw string key
- WHEN a subsequent call changes descriptor, source, display or must-hit metadata
- THEN the call is rejected without consuming the original site's sparse outcome sample

#### Scenario: Genuine outcomes survive rejected attempts

- GIVEN rejected same-key metadata changes and an exact original registration
- WHEN legitimate passing and failing observations are emitted
- THEN original IDs, fingerprints, source fields and actual conditions are retained, with default per-hit or sparse first-outcome counts as configured

### Requirement: Runtime raw string IDs have independent accepted identities

r[chaoscontrol.raw_string_ids.independent]

Runtime raw assertions MUST register alias-free stable descriptors and resolve observations by their exact namespace/logical string key. Distinct admitted IDs whose historical 32-bit hashes collide MUST retain separate catalog entries, fingerprints, raw output IDs, counts and sparse-sampling state. Runtime raw optional-always flags, reachable/forbidden semantics and late-site rejection MUST remain unchanged. Compiled compatibility aliases MUST remain unchanged.

#### Scenario: Two colliding string IDs execute independently

- GIVEN two raw declarations with distinct IDs, colliding historical hashes and identical kind/message
- WHEN both properties emit passing and failing observations
- THEN each observation binds its own descriptor and each property retains its own default or sparse counts

#### Scenario: Strict quality keeps failures separated

- GIVEN actual local raw output from two such properties
- WHEN strict report and quality CLIs evaluate all-passing and one-property-failing runs
- THEN passing runs are accepted and the genuine failing property is rejected without attributing its violation to the other ID
