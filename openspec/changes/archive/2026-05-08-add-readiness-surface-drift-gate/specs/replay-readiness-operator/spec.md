## ADDED Requirements

### Requirement: Generated readiness surface drift gate [r[generated-readiness-surface-drift-gate]]
The replay-readiness static gate MUST fail closed when the receipt, one-line summary, dashboard, README status snippet, or static gate metadata diverges from the same replay-readiness source of truth.

#### Scenario: Static gate metadata matches executed gates [r[generated-readiness-surface-drift-gate.static-gates]]
- GIVEN the replay-readiness shell executes a set of static `run_gate` checks
- WHEN the generated-surface drift gate runs
- THEN every executed static gate appears in the receipt `static_gates` metadata exactly once
- AND no receipt static gate is listed without a corresponding executed gate

#### Scenario: Summary surfaces share one summary line [r[generated-readiness-surface-drift-gate.summary-line]]
- GIVEN a valid replay-readiness receipt fixture
- WHEN the summary, dashboard, and README status renderers run
- THEN the dashboard and README snippet contain the exact summary line produced by the summary renderer
- AND both surfaces preserve bounded-scope anti-overclaim language

#### Scenario: Missing renderer marker fails closed [r[generated-readiness-surface-drift-gate.marker-fails]]
- GIVEN a README input without the replay-readiness status marker block
- WHEN the generated-surface drift gate exercises the README updater
- THEN it exits nonzero instead of silently accepting a stale or missing status surface
