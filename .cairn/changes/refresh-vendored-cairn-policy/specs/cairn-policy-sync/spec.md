# Cairn Policy Sync Specification

## Purpose

Keep the vendored Cairn policy loadable by the pinned Cairn build and traceable to one upstream revision.

## ADDED Requirements

### Requirement: Policy sources come from one pinned revision

r[chaoscontrol.cairn_policy_sync.pinned_import] Every vendored Cairn policy file in the overlay's import closure MUST match one recorded full Cairn commit hash, unless `cairn-policy/UPSTREAM.md` lists it as ChaosControl-owned.

#### Scenario: One imported file is stale

- GIVEN an imported policy file that differs from the recorded revision
- WHEN the closure check runs
- THEN it MUST fail and name the file.

### Requirement: The committed projection is fresh

r[chaoscontrol.cairn_policy_sync.projection] The committed projection MUST equal a fresh export of the ChaosControl overlay made with the Cairn build at the recorded revision.

#### Scenario: Projection is regenerated

- GIVEN the pinned Cairn build and the committed overlay
- WHEN the projection is exported again
- THEN the output MUST equal the committed projection.

### Requirement: Validation runs on active changes

r[chaoscontrol.cairn_policy_sync.validation] The pinned Cairn build MUST load the committed policy, and `cairn validate --root .` MUST complete. Every reported finding on an active change MUST be fixed or recorded with a reason.

#### Scenario: Policy lacks a required workflow profile

- GIVEN a projection that lacks a workflow profile the pinned build requires
- WHEN any policy-loading command runs
- THEN the command MUST fail, and this change MUST NOT be marked complete.
