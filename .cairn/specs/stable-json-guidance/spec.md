# Stable Json Guidance Specification

## Purpose

Defines the `stable-json-guidance` capability.

## Requirements

### Requirement: Stable JSON guidance uses full catalog identity

r[chaoscontrol.stable_json_guidance.binding] Stable JSON guidance MUST resolve the accepted namespace, logical key, kind and message rather than the compact compatibility alias. Distinct admitted sites sharing an alias MUST retain independently attributed JSON guidance observations.

#### Scenario: Cross-namespace aliases collide

- GIVEN two accepted stable assertions with the same compact alias, kind and message but distinct namespaces and logical keys
- WHEN each site emits JSON guidance
- THEN both observations are retained with the corresponding accepted catalog token and fingerprint, original data, direction and caller location.

### Requirement: Unresolved stable guidance fails closed

r[chaoscontrol.stable_json_guidance.rejection] Stable JSON guidance MUST emit no observation when namespace, logical key, kind or message does not match an accepted stable assertion. Existing compatibility guidance APIs and record schema MUST remain unchanged.

#### Scenario: A stable guidance descriptor does not match

- GIVEN an accepted stable assertion catalog
- WHEN guidance supplies an unknown namespace or key, or the wrong kind or message
- THEN no JSON guidance observation is emitted for that request.
