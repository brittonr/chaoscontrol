# Release preparation delta

## ADDED Requirements

### Requirement: Preserve lifecycle obligations

r[chaoscontrol.release_preparation.lifecycle] Release preparation MUST repair invalid task linkage and synchronize reviewed implemented requirement deltas without marking incomplete campaign or implementation tasks complete, deleting genuine requirements, or weakening release policy.

#### Scenario: A campaign remains incomplete

- GIVEN a change with implemented local mechanisms and an unexecuted candidate campaign
- WHEN its reviewed requirements are synchronized
- THEN the campaign task and active change remain open and no campaign result is inferred

### Requirement: Separate public scope from production readiness

r[chaoscontrol.release_preparation.boundary] Public-facing guidance MUST identify supported scope, release checks, licensing and security boundaries and MUST distinguish current observations from historical release receipts.

#### Scenario: A historical gate passed

- GIVEN a passing receipt on an earlier revision
- WHEN release guidance is presented
- THEN its original revision and verdict remain intact and it does not promote the current candidate

### Requirement: Evidence-bound release preparation

r[chaoscontrol.release_preparation.evidence] Release preparation MUST record actual command outcomes and missing prerequisites, and MUST NOT claim production readiness without the existing selected-policy and current-source release obligations.

#### Scenario: Required evidence is absent

- GIVEN missing current-source completion or KVM evidence
- WHEN preparation checks finish
- THEN production readiness remains blocked with the missing evidence named
