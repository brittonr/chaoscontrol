## Context

The SDK already supports assertions, lifecycle events, guided randomness, local JSONL output, workload helpers, VM campaigns, and accepted Rust workload proof. The missing product feature is an integrated, low-friction Rust path.

## Goals / Non-Goals

**Goals:**
- One documented/scaffolded path from a Rust crate to local report and VM campaign.
- Deterministic checks for assertion quality before expensive campaigns.
- A promotion checklist that preserves local-vs-replay evidence boundaries.

**Non-Goals:**
- SDKs for non-Rust languages.
- Docker/OCI/Compose onboarding.
- Hosted product setup.

## Decisions

### 1. Scaffold emits contracts, not magic

**Choice:** Generate a small Rust harness/template plus explicit config/report files instead of hiding behavior in a wizard.
**Rationale:** Users can inspect commands, workload names, VM counts, and evidence classes.
**Alternative:** Add a broad CLI wizard. Rejected for this slice because deterministic files are easier to test and review.
**Implementation:** Add a scaffold/template command or Nix app, local-report check modes, docs examples, and CI fixtures.

### 2. Assertion linting runs before VM proof

**Choice:** Add checks for uncategorized assertions, missing lifecycle setup, sometimes-without-success, and no observed random/guidance sites when expected.
**Rationale:** VM campaigns are more valuable after local instrumentation is credible.
**Alternative:** Let replay-readiness discover weak instrumentation later. Rejected because it wastes KVM time.

## Risks / Trade-offs

**Template drift** → Gate generated examples and docs with a check that runs the scaffolded sample.
