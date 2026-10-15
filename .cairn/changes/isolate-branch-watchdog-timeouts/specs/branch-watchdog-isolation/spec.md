# Branch Watchdog Isolation Specification

## Purpose

Keep one operational host watchdog timeout from discarding a whole exploration campaign. Interrupted branches must not appear as exploration evidence.

## ADDED Requirements

### Requirement: Branch watchdog timeouts are isolated

r[chaoscontrol.branch_watchdog.isolation] When a branch run returns `HostWatchdogTimeout`, the explorer MUST handle it as an operational branch timeout and continue the round. Any other branch or bootstrap error MUST keep its existing handling.

#### Scenario: One branch times out

- GIVEN a round of eight branches in which one branch returns `HostWatchdogTimeout`
- WHEN the round completes
- THEN the other seven branch results MUST be processed
- AND the campaign MUST continue to the next round.

#### Scenario: A non-watchdog VM error occurs

- GIVEN a sequential branch that returns a VM error other than `HostWatchdogTimeout`
- WHEN the round executes
- THEN the error MUST propagate as it did before this change.

### Requirement: Poisoned controllers are replaced

r[chaoscontrol.branch_watchdog.recovery] After a branch watchdog timeout, the explorer MUST NOT reuse the poisoned controller. It MUST build a replacement controller whose guest memory equals the bootstrap base before the next incremental restore.

#### Scenario: Branch after recovery

- GIVEN a controller poisoned by a branch watchdog timeout
- WHEN the next branch runs
- THEN the branch MUST run on a replacement controller restored from the retained bootstrap state.

### Requirement: Timed-out branches retry within a bound

r[chaoscontrol.branch_watchdog.retry] A timed-out branch that has a parent snapshot MUST be retried from that snapshot until it completes or reaches `MAX_BRANCH_WATCHDOG_ATTEMPTS`. A branch without a parent snapshot MUST NOT be retried.

#### Scenario: Retry succeeds

- GIVEN a branch that times out once and then completes
- WHEN the round processes results
- THEN the completed attempt MUST be the branch result
- AND the round MUST record one watchdog firing and zero discarded branches.

#### Scenario: Attempt bound reached

- GIVEN a branch that times out on every attempt
- WHEN the attempt bound is reached
- THEN the branch MUST be discarded.

### Requirement: Discarded branches contribute nothing

r[chaoscontrol.branch_watchdog.discard] A discarded branch MUST NOT merge coverage, extract bugs, add a corpus or frontier entry, or increment the branch count.

#### Scenario: Discarded branch state

- GIVEN a branch discarded after its attempts
- WHEN the explorer processes the round
- THEN global coverage, bugs, corpus, frontier, and branch totals MUST be unchanged by that branch.

### Requirement: All branch paths share the handling

r[chaoscontrol.branch_watchdog.paths] The sequential, worker-pool, and input-tree paths MUST apply the same classification, recovery, retry, and discard rules. The worker pool MUST NOT convert a watchdog timeout into a placeholder branch result.

#### Scenario: Input-tree probe is discarded

- GIVEN an input-tree round whose probe branch is discarded
- WHEN the round executes
- THEN the round MUST run no override branches.

### Requirement: Timeouts are reported as operational evidence

r[chaoscontrol.branch_watchdog.evidence] Each round MUST record watchdog firings and discarded branches in `RoundHistory`, the checkpoint round history, and metrics output. The human report MUST state nonzero totals, and it MUST say that a nonzero discard count means the exploration trajectory depended on host timing. Operator scripts MUST NOT present an explorer error as a found bug.

#### Scenario: Old checkpoint loads

- GIVEN a checkpoint whose round history lacks the watchdog fields
- WHEN it is loaded
- THEN every round MUST report zero firings and zero discards.

### Requirement: An all-discard round fails closed

r[chaoscontrol.branch_watchdog.fail_closed] If a round attempts at least one branch and discards every attempted branch, the explorer MUST return `HostWatchdogTimeout`.

#### Scenario: Every branch is discarded

- GIVEN a round in which every attempted branch exhausts its watchdog attempts
- WHEN the round completes
- THEN the campaign MUST stop with `HostWatchdogTimeout` rather than record an empty round.
