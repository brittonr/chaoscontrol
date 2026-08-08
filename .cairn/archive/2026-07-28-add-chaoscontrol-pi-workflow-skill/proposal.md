## Why

ChaosControl already has bounded Rust workload, campaign, replay, and triage rails. Pi lacks a repo-owned workflow that sequences these rails without importing Antithesis product assumptions.

## What Changes

- Add an Agent Skills-compatible `chaoscontrol-workflow` source under `docs/skills/`.
- Split research, workload onboarding, campaign, and triage instructions into focused reference files.
- Preserve lifecycle, evidence-class, replay, and anti-overclaim boundaries in every workflow.
- Document the Antithesis skills repository as workflow-design prior art, not a runtime dependency.
- Install the repo-owned skill into the global Pi skill directory through a local symbolic link.

## Capabilities

### New Capabilities

- `agent-workflow-skill`: Provides a bounded Pi workflow for ChaosControl research, Rust workload onboarding, campaigns, replay, and triage.

### Modified Capabilities

None.

## Impact

The change adds documentation and a Pi skill source. It does not change the ChaosControl runtime, SDK, evidence schemas, or product scope.
