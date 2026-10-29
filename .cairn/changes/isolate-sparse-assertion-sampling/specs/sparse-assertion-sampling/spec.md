## ADDED Requirements

### Requirement: Sparse tracking uses accepted identity

r[chaoscontrol.sparse_sampling.identity] Sparse assertion emission MUST use the admitted assertion fingerprint for tracking, shared across stable and compatibility call paths. Distinct admitted descriptors MUST NOT suppress one another because their compact aliases collide. Unresolved assertions MUST NOT populate sparse tracking.

#### Scenario: Colliding aliases across namespaces

- GIVEN two accepted assertions with the same compact alias in separate namespaces
- WHEN stable and compatibility entry points emit their outcomes
- THEN each fingerprint retains its own first pass and first failure.

### Requirement: Outcome sampling remains opt-in

r[chaoscontrol.sparse_sampling.outcomes] With `sparse_emission`, each admitted identity MUST emit at most one hit per boolean outcome without counter overflow. Catalog declarations MUST remain unsampled. Without that feature, assertions MUST retain per-hit emission.

#### Scenario: Repeated outcomes

- GIVEN repeated true and false evaluations of admitted assertions
- WHEN sparse mode is enabled or disabled
- THEN sparse mode retains each first outcome and default mode retains every evaluation, while declarations remain visible.
