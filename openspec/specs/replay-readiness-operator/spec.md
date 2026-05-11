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

### Requirement: Static fleet triage index [r[static-fleet-triage-index]]

The system MUST render one or more replay-readiness receipts into a static multi-receipt fleet triage index without claiming hosted service, scheduler, shared decision-store, or full product parity support.

#### Scenario: Index renders multiple receipt summaries [r[static-fleet-triage-index.render]]

- GIVEN one or more valid replay-readiness receipt paths
- WHEN the fleet triage index renderer is invoked with an output path
- THEN it writes a static HTML artifact listing each receipt path, status, selected workload, replay class, replay-parent depth, and stable summary line

#### Scenario: Empty receipt set fails closed [r[static-fleet-triage-index.empty-fails]]

- GIVEN no replay-readiness receipt paths
- WHEN the fleet triage index renderer is invoked
- THEN it exits nonzero instead of emitting an empty or misleading fleet artifact

#### Scenario: Hosted fleet parity remains unpromoted [r[static-fleet-triage-index.anti-overclaim]]

- GIVEN a generated static fleet triage index
- WHEN an operator reviews the artifact
- THEN the artifact states that it is bounded static multi-receipt review, not universal replay evidence, hosted service, scheduler integration, shared decision store, or full Antithesis-style product replacement

### Requirement: Local replay-readiness decision receipt [r[local-decision-receipt]]

The system MUST provide a bounded local replay-readiness decision receipt format that records operator decisions for fleet-style triage without claiming hosted UI, scheduler integration, shared decision-store support, raw-log scraping, or full product parity.

#### Scenario: Decision receipt validates local decisions [r[local-decision-receipt.validate]]

- GIVEN a decision receipt with schema version, source fleet index, source replay-readiness receipt paths, non-empty decisions, linked artifacts, local scope, and anti-claims
- WHEN the decision receipt validator runs
- THEN it returns a stable summary containing the recorded status, decision count, actions, receipt count, and bounded-local-not-shared scope token

#### Scenario: Unsafe decision receipt fails closed [r[local-decision-receipt.fail-closed]]

- GIVEN a decision receipt that enables raw-log scraping, omits source receipts, duplicates decision IDs, uses unsupported actions, or weakens the bounded local anti-claim language
- WHEN the decision receipt validator runs
- THEN it exits nonzero and does not accept the receipt as fleet triage evidence

#### Scenario: Decision receipt is packaged with readiness artifacts [r[local-decision-receipt.packaged]]

- GIVEN the replay-readiness Nix check succeeds
- WHEN the check output is inspected
- THEN it contains a non-empty decision receipt and decision receipt summary alongside the readiness receipt, summary, dashboard, runbook, and fleet index artifacts

### Requirement: Local replay scheduler receipt [r[local-scheduler-receipt]]

The system MUST provide a bounded local replay scheduler receipt format that records a manual multi-run replay-readiness plan without claiming hosted service, fleet-scale scheduler, shared queue, cross-machine orchestration, raw-log scraping, or full product parity.

#### Scenario: Scheduler receipt validates local run plans [r[local-scheduler-receipt.validate]]

- GIVEN a scheduler receipt with schema version, local scope, bounded schedule, non-empty run plan, unique run IDs, workload names, commands, receipt paths, decision policies, and anti-claims
- WHEN the scheduler receipt validator runs
- THEN it returns a stable summary containing recorded status, run count, workloads, schedule mode, and bounded-local-not-hosted scope token

#### Scenario: Unsafe scheduler receipt fails closed [r[local-scheduler-receipt.fail-closed]]

- GIVEN a scheduler receipt that enables raw-log scraping, duplicates run IDs, exceeds max runs, uses unsupported schedule modes, uses unsupported decision policies, or weakens the not-hosted/not-fleet/not-shared-queue anti-claim language
- WHEN the scheduler receipt validator runs
- THEN it exits nonzero and does not accept the receipt as scheduler orchestration evidence

#### Scenario: Scheduler receipt is packaged with readiness artifacts [r[local-scheduler-receipt.packaged]]

- GIVEN the replay-readiness Nix check succeeds
- WHEN the check output is inspected
- THEN it contains a non-empty scheduler receipt and scheduler receipt summary alongside the readiness receipt, dashboard, runbook, fleet index, and decision receipt artifacts

### Requirement: Local replay scheduler execution receipt [r[local-scheduler-execution-receipt]]

The system MUST provide a bounded local sequential scheduler execution receipt that runs or records multiple scheduler-plan commands, links each successful run to a replay-readiness receipt summary, and does not claim hosted scheduling, fleet-scale workers, shared queues, cross-machine orchestration, raw-log scraping, or product parity.

#### Scenario: Scheduler execution records receipt-backed runs [r[local-scheduler-execution-receipt.records-runs]]

- GIVEN a valid local scheduler receipt with `concurrency=1` and multiple run-plan entries
- WHEN the local scheduler execution command runs the plan
- THEN it writes an execution receipt with one run result per entry, command, exit code, receipt path, stable receipt summary for passed runs, and bounded-local-sequential scope

#### Scenario: Scheduler execution fails closed on overclaims [r[local-scheduler-execution-receipt.fail-closed]]

- GIVEN a scheduler execution receipt that enables raw-log scraping, declares hosted or fleet/shared-queue semantics, duplicates run IDs, omits successful receipt summaries, or records concurrency greater than one
- WHEN the scheduler execution validator runs
- THEN it rejects the receipt and does not accept it as scheduler orchestration evidence

#### Scenario: Scheduler execution is packaged with readiness artifacts [r[local-scheduler-execution-receipt.packaged]]

- GIVEN the replay-readiness Nix check succeeds
- WHEN the check output is inspected
- THEN it contains a non-empty scheduler execution plan, scheduler execution receipt, scheduler execution summary, and linked per-run replay-readiness receipts
