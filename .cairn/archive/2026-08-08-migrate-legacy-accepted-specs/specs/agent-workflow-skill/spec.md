# agent-workflow-skill Specification

## Purpose
Define the repository-owned agent workflow that preserves ChaosControl lifecycle, workload-scope, evidence-class, and receipt-first triage boundaries.
## Requirements
### Requirement: Discoverable ChaosControl workflow skill
The repository MUST provide an Agent Skills-compatible `chaoscontrol-workflow` source with focused research, workload, campaign, and triage references.

#### Scenario: Pi discovers the installed skill
- **WHEN** the canonical skill directory is linked into a global Pi skill location
- **THEN** the skill frontmatter supplies the exact `chaoscontrol-workflow` name and a specific trigger description

#### Scenario: Missing reference blocks completion
- **WHEN** the main skill names a stage reference that does not exist
- **THEN** the source audit reports the missing reference and the installation remains incomplete

### Requirement: Repository authority and lifecycle boundaries
The workflow MUST read target repository instructions before mutation and MUST use the lifecycle system selected by those instructions.

#### Scenario: Target repository uses a reviewed lifecycle
- **WHEN** the workflow needs to change product source or durable configuration
- **THEN** it creates or continues the required Cairn change before implementation

#### Scenario: Candidate research is not accepted authority
- **WHEN** research produces a candidate property catalog
- **THEN** the workflow keeps the catalog as review-needed evidence instead of an accepted requirement or correctness claim

### Requirement: Staged Rust workload workflow
The workflow MUST sequence external research, workload onboarding, local instrumentation, VM campaigns, and replay promotion without Docker or hosted-service assumptions.

#### Scenario: New Rust workload starts with the external harness
- **WHEN** a target project begins ChaosControl adoption
- **THEN** the workflow starts with the scaffold and local dry-run before it proposes in-process instrumentation

#### Scenario: Unsupported onboarding request stops
- **WHEN** a request requires a non-Rust SDK, Docker Compose, Kubernetes, or a hosted Antithesis service
- **THEN** the workflow reports that the request is outside the current ChaosControl product scope

### Requirement: Evidence classes remain separate
The workflow MUST distinguish instrumentation evidence, VM execution evidence, and snapshot-backed replay evidence.

#### Scenario: Snapshot-backed reproduction supports promotion
- **WHEN** a replay verdict reports `snapshot_backed_reproduced` with valid retained parent-snapshot evidence
- **THEN** the workflow can cite the bounded selected replay claim and its artifacts

#### Scenario: Weak evidence cannot support replay
- **WHEN** only a local dry-run, raw log, schedule-only result, or bounded campaign result exists
- **THEN** the workflow reports the exact evidence gap and does not claim replay proof

### Requirement: Receipt-first triage
The workflow MUST start triage from bounded receipts and linked bug artifacts before it uses debug logs.

#### Scenario: Triage records a bounded decision
- **WHEN** reproduce and minimize complete for a selected bug artifact
- **THEN** the workflow records the result in the repository-supported decision receipt format

#### Scenario: Raw logs cannot become acceptance evidence
- **WHEN** a run has only unbounded or unlinked raw logs
- **THEN** the workflow keeps those logs as local debug aids and reports that acceptance evidence is missing
