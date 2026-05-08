## ADDED Requirements

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
