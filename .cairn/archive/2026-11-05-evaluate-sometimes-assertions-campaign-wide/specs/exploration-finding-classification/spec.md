# Exploration Finding Classification Specification

## Purpose

Separate safety violations found in one branch from liveness properties that only a whole campaign can decide.

## ADDED Requirements

### Requirement: Branch bugs are safety violations

r[chaoscontrol.exploration_findings.safety_bugs] The explorer MUST create a branch bug record only when an `always` assertion is observed false or an `unreachable` assertion is reached in that branch lineage.

#### Scenario: Always assertion fails

- GIVEN a branch lineage in which an `always` assertion is observed false
- WHEN bugs are extracted
- THEN the explorer MUST create a bug record for that assertion.

#### Scenario: Sometimes assertion unsatisfied in one branch

- GIVEN a branch lineage in which a `sometimes` assertion was hit but never true
- WHEN bugs are extracted
- THEN the explorer MUST NOT create a bug record.

### Requirement: Liveness assertions are evaluated per campaign

r[chaoscontrol.exploration_findings.campaign_liveness] At the end of a campaign, a `sometimes` assertion that was hit but never true in any branch, and a `reachable` assertion that no branch reached, MUST be reported as coverage gaps. Coverage gaps MUST NOT be counted as bugs.

#### Scenario: One branch satisfies the assertion

- GIVEN a `sometimes` assertion that is false in seven branches and true in one
- WHEN the campaign ends
- THEN the assertion MUST NOT appear as a coverage gap or a bug.

#### Scenario: No branch satisfies the assertion

- GIVEN a `sometimes` assertion that no branch observed true
- WHEN the campaign ends
- THEN the report MUST list it as a coverage gap.

### Requirement: Reports show distinct failures

r[chaoscontrol.exploration_findings.distinct_counts] The report MUST show the number of distinct failing assertion identities next to the number of bug records.

#### Scenario: One assertion fails under many fault sets

- GIVEN one `always` assertion that fails under several fault-type sets
- WHEN the report is written
- THEN it MUST show one distinct failing assertion and the full bug-record count.

### Requirement: Safety replay contracts are unchanged

r[chaoscontrol.exploration_findings.compatibility] Bug records for safety violations MUST keep their current identity, dedup key, replay-parent binding, and replay verdict contracts.

#### Scenario: Existing safety bug replays

- GIVEN a safety bug record produced before this change
- WHEN it is replayed
- THEN replay admission MUST apply the same checks as before.
