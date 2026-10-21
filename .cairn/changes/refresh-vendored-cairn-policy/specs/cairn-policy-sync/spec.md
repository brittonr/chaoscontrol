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

### Requirement: Consumer MCP completion preserves genuine obligations

r[chaoscontrol.cairn_policy_sync.consumer_mcp] The compatible cohort MUST support ChaosControl-owned completion contract, profiles, source projection, nonempty observed dependency identities and explicit typed readiness selection. The contract MUST preserve every portable command in `fixtures/agent-completion/registry.ncl` with its exact command identity. Rust MUST produce execution-result rows; authored passing rows or copied Cairn self-fixtures MUST NOT substitute. Independent native and seven-row KVM release gates, historical retention and replay acceptance MUST remain required.

#### Scenario: A foreign self-host gate is inherited

- GIVEN Cairn's default gate summary names its own `slotcar` installable and ChaosControl has no such gate
- WHEN the consumer selects its reviewed typed readiness policy
- THEN the report MUST name the selected consumer identity and complete genuine portable command list, without treating the foreign installable as a ChaosControl obligation or suppressing a genuine required check.

#### Scenario: Producer cannot represent a maintained command

- GIVEN a required command, dependency identity set, explicit environment or execution bound that the selected producer grammar cannot admit
- WHEN preparation or planning runs
- THEN completion MUST remain nonpassing with the exact mismatch recorded, rather than substituting a smaller Cargo smoke, omitting identities or fabricating a result.

#### Scenario: MCP process exits zero with a failed report

- GIVEN real failed or stale producer rows, or an invalid present consumer selection
- WHEN read-only MCP or native readiness validates those inputs
- THEN the failure verdict MUST remain authoritative regardless of process exit status, with the exact selected policy applied and no fallback to self-host defaults.
