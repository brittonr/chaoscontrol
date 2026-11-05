# Explorer Operator Outcomes Specification

## Purpose

Let operators and scripts separate explorer failures from exploration findings.

## ADDED Requirements

### Requirement: Failures have a dedicated exit status

r[chaoscontrol.explorer_outcomes.exit_status] Exploration commands MUST exit with a dedicated failure status when they fail. Clean runs MUST exit 0, runs with bugs MUST exit 1, and assertion-floor failures MUST exit 3. Operator scripts MUST classify results from these statuses.

#### Scenario: Exploration fails

- GIVEN a `run` command whose exploration returns an error
- WHEN the process exits
- THEN the status MUST be the dedicated failure status, not 1.

#### Scenario: Exploration finds a bug

- GIVEN a completed `run` with at least one bug record
- WHEN the process exits
- THEN the status MUST be 1.

### Requirement: Errors show their source chain

r[chaoscontrol.explorer_outcomes.error_chain] When an explorer command fails, it MUST print the top-level error and every source error in order.

#### Scenario: Minimization hits a restore failure

- GIVEN a minimization whose snapshot restore fails with a virtio queue error
- WHEN the command reports the failure
- THEN the output MUST include the virtio queue error text.

### Requirement: Failed pool branches are not evidence

r[chaoscontrol.explorer_outcomes.failed_branches] A worker-pool branch that errors or panics for a reason other than the host watchdog MUST be recorded as a failed branch. It MUST NOT contribute coverage, bugs, frontier or corpus entries, or branch counts. A round whose attempted branches all failed or were discarded MUST fail closed.

#### Scenario: One pool branch panics

- GIVEN a round in which one worker branch panics
- WHEN the round completes
- THEN the round history MUST record one failed branch
- AND global coverage MUST be unchanged by that branch.
