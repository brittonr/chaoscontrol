## ADDED Requirements

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
