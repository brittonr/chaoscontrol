## ADDED Requirements

### Requirement: Local campaign fault coverage summary [r[local-campaign-fault-coverage-summary]]

Local multi-hypervisor campaign receipts MUST summarize deterministic fault coverage by workload and fault class without relying on raw-log scraping or claiming exhaustive fault coverage.

#### Scenario: Campaign receipt records exercised fault classes [r[local-campaign-fault-coverage-summary.records-classes]]

- GIVEN a local multi-hypervisor campaign runs with deterministic network, block, timer, process, or scheduler fault policies
- WHEN the campaign receipt is emitted
- THEN it records configured fault classes, injection attempts, observed injections or explicit not-observed status, affected run IDs, and unsupported classes

#### Scenario: Fault coverage anti-claim is preserved [r[local-campaign-fault-coverage-summary.anti-claim]]

- GIVEN the fault coverage summary is rendered in readiness docs or dashboard output
- WHEN an operator reviews it
- THEN it states that coverage is limited to the listed fault classes and workloads and is not exhaustive validation of all possible failures
