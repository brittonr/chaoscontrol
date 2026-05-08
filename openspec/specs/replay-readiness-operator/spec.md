# replay-readiness-operator Specification

## Purpose

Defines the operator-facing replay-readiness receipt surfaces used by CI and local dashboard review while preserving bounded evidence claims.
## Requirements
### Requirement: Replay-readiness dashboard artifact [r[replay-readiness-dashboard-artifact]]

The system MUST render a replay-readiness receipt into a self-contained static HTML dashboard artifact for CI and local operator review.

#### Scenario: Dashboard renders a passing checks-only receipt [r[replay-readiness-dashboard-artifact.checks-only]]

- GIVEN a valid replay-readiness receipt with all static gates passing and no selected dogfood workload
- WHEN the dashboard renderer is invoked with an output path
- THEN it writes a static HTML file containing final status, static gate counts, dogfood skipped status, and bounded scope text

#### Scenario: Dashboard renders selected dogfood evidence [r[replay-readiness-dashboard-artifact.dogfood]]

- GIVEN a valid replay-readiness receipt with a selected dogfood workload and summary verdict
- WHEN the dashboard renderer is invoked
- THEN the HTML includes the workload, expectation status, acceptance status, replay class, replay-parent depth, seed, and fail-after value

#### Scenario: Dashboard fails closed on malformed receipts [r[replay-readiness-dashboard-artifact.fail-closed]]

- GIVEN a malformed or non-replay-readiness receipt
- WHEN the dashboard renderer is invoked
- THEN it exits nonzero and does not emit a misleading dashboard artifact

### Requirement: Dashboard CI packaging [r[replay-readiness-dashboard-ci]]

The replay-readiness CI/check surface MUST package the generated dashboard next to the receipt JSON and one-line summary so operators can inspect the same evidence without raw log scraping.

#### Scenario: Nix check emits dashboard artifact [r[replay-readiness-dashboard-ci.nix-check]]

- GIVEN the replay-readiness Nix check succeeds
- WHEN the check output is inspected
- THEN it contains non-empty `replay-readiness-receipt.json`, `replay-readiness-summary.txt`, and `replay-readiness-dashboard.html` artifacts

### Requirement: README replay-readiness status snippet [r[replay-readiness-readme-status]]

The system MUST provide a deterministic README status snippet derived from a validated replay-readiness receipt summary so the repository front page exposes the current bounded readiness claim without requiring CI artifact inspection.

#### Scenario: README status refreshes from a valid receipt [r[replay-readiness-readme-status.refresh]]

- GIVEN a README containing the replay-readiness status marker block and a valid replay-readiness receipt
- WHEN the README status updater is invoked
- THEN only the marker-bounded status block is replaced with a Markdown snippet containing the stable summary line and bounded scope language

#### Scenario: README status updater fails closed [r[replay-readiness-readme-status.fail-closed]]

- GIVEN a malformed receipt or a README missing the marker block
- WHEN the README status updater is invoked
- THEN it exits nonzero and does not emit a misleading status update

### Requirement: Fresh workload promotion gate [r[fresh-workload-promotion-gate]]
The replay-readiness static gate MUST fail closed when a workload is presented as supported without an accepted manifest proof, bounded anti-claims, and the generated experimental/unproven surface classifications that prevent overclaiming fresh workload authoring.

#### Scenario: Accepted manifest and report agree [r[fresh-workload-promotion-gate.manifest-report-agree]]
- GIVEN the accepted workload proof manifest and generated readiness status report
- WHEN the promotion gate runs
- THEN every supported workload row maps to exactly one accepted manifest proof with a unique assertion ID
- AND fresh workload authoring remains classified as experimental until a new accepted proof is committed

#### Scenario: Report-only promotion fails closed [r[fresh-workload-promotion-gate.report-only-fails]]
- GIVEN a readiness status report that lists a supported workload missing from the accepted manifest
- WHEN the promotion gate runs
- THEN it exits nonzero and reports the unsupported workload promotion

#### Scenario: Anti-claim removal fails closed [r[fresh-workload-promotion-gate.anti-claim-fails]]
- GIVEN an accepted manifest with missing or weakened anti-claim text
- WHEN the promotion gate runs
- THEN it exits nonzero before the readiness surface can be promoted

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
