# nickel-evidence-contracts Specification

## Purpose

This spec defines the Nickel-backed configuration and evidence contract boundary for ChaosControl dogfood and exploration receipts. It records which artifacts are Nickel-authored, which runtime evidence shapes remain Rust-owned but validated, and which raw/debug/security-sensitive surfaces are excluded from acceptance evidence.

## Requirements
### Requirement: Nickel-backed run configuration [r[nickel-evidence-contracts.run-config]]
The system SHALL support human-authored Nickel run configuration for exploration and dogfood runs, with contracts for guest selection, VM count, tick budget, branch/round limits, fault model, output policy, and replay policy before any long-running exploration begins.

#### Scenario: Valid config exports to explorer JSON [r[nickel-evidence-contracts.run-config.valid-export]]
- **GIVEN** a Nickel run configuration that satisfies the run-config contract
- **WHEN** the validation/export command runs
- **THEN** it writes deterministic JSON consumable by the existing Rust explorer
- **AND** the exported config records enough metadata to bind later receipts to the validated input

#### Scenario: Invalid config fails before execution [r[nickel-evidence-contracts.run-config.invalid-fails-before-run]]
- **GIVEN** a Nickel run configuration with an invalid VM count, missing guest artifact, or incompatible replay policy
- **WHEN** a dogfood or campaign command attempts to use it
- **THEN** validation fails before starting VMs or writing run artifacts

### Requirement: Source-of-truth registry [r[nickel-evidence-contracts.source-of-truth-registry]]
The system SHALL maintain a registry that classifies each configuration and evidence artifact family as `nickel-authored`, `rust-derived`, or excluded from Nickel validation, and each entry SHALL name its owner, validation command, fixture coverage, and freshness expectation.

#### Scenario: Runtime records remain Rust-owned [r[nickel-evidence-contracts.source-of-truth-registry.rust-owned-records]]
- **GIVEN** runtime-emitted records such as bug reports, assertion summaries, campaign progress, and checkpoints
- **WHEN** the registry is inspected
- **THEN** those records are marked `rust-derived` or Rust-owned evidence shapes rather than hand-authored Nickel sources
- **AND** validation uses their serialized public evidence shape without creating a competing source of truth

#### Scenario: Excluded surfaces are explicit [r[nickel-evidence-contracts.source-of-truth-registry.exclusions]]
- **GIVEN** raw logs, secrets, cryptographic internals, wire discriminants, or hot-path runtime constants
- **WHEN** the registry is checked
- **THEN** those surfaces are explicitly excluded from Nickel ownership or committed receipt content

### Requirement: Validated dogfood receipts [r[nickel-evidence-contracts.dogfood-receipts]]
The system SHALL validate dogfood receipts that bind command, git revision, validated config digest, kernel/initrd artifacts, artifact paths and hashes, assertion coverage, bug reports, replay attempts, acceptance status, and known gaps.

#### Scenario: Reported bug requires replay status [r[nickel-evidence-contracts.dogfood-receipts.reported-bug-replay-status]]
- **GIVEN** a receipt that references one or more `bug_*.json` files
- **WHEN** receipt validation runs
- **THEN** every bug has a linked replay attempt with command, result, and status
- **AND** a failed replay is classified as `known-gap` or `invalid`, not accepted reproduction evidence

#### Scenario: Artifact hashes bind receipt content [r[nickel-evidence-contracts.dogfood-receipts.artifact-hashes]]
- **GIVEN** a receipt that names report, assertion, checkpoint, or bug artifacts
- **WHEN** validation runs
- **THEN** each required artifact exists and its digest matches the receipt
- **AND** stale or missing artifacts make the receipt invalid

#### Scenario: Raw logs remain optional references [r[nickel-evidence-contracts.dogfood-receipts.raw-logs]]
- **GIVEN** raw `run.log` or `reproduce.log` files exist locally
- **WHEN** a receipt is validated for review
- **THEN** the receipt may reference those logs as local debug aids
- **AND** validation does not require committing voluminous raw logs as acceptance evidence

### Requirement: Contract validation fixtures and gates [r[nickel-evidence-contracts.validation-gates]]
The system SHALL include positive and negative fixtures plus local/Nix validation gates that prove the Nickel contracts accept valid evidence and reject incomplete or stale evidence.

#### Scenario: Existing Raft dogfood receipt validates as known gap [r[nickel-evidence-contracts.validation-gates.raft-known-gap]]
- **GIVEN** the committed Raft dogfood artifacts under `dogfood-results/raft-20260506-095025/`
- **WHEN** the contract validation gate runs
- **THEN** the artifacts validate structurally
- **AND** the failed standalone replay is preserved as a `known-gap` status rather than accepted reproduction evidence

#### Scenario: Negative fixture rejects missing replay context [r[nickel-evidence-contracts.validation-gates.missing-replay-context]]
- **GIVEN** a fixture with a reported bug but no replay attempt or deterministic replay context
- **WHEN** contract validation runs
- **THEN** validation fails with an error identifying the missing replay evidence

#### Scenario: CI/local check covers contract freshness [r[nickel-evidence-contracts.validation-gates.freshness]]
- **GIVEN** Rust-owned evidence record shapes or generated Nickel contracts change
- **WHEN** the local validation bundle runs
- **THEN** stale generated contracts or mismatched fixtures fail the check before receipts are accepted
