---
name: chaoscontrol-workflow
description: Research, onboard, run, replay, and triage Rust workloads with the local ChaosControl deterministic VMM and evidence rails. Use for ChaosControl properties, SDK assertions, workload scaffolds, fault campaigns, replay verdicts, and readiness receipts.
compatibility: Linux x86_64 with Nix. KVM access is required for VM campaigns and replay.
metadata:
  version: "1.0.0"
---

# ChaosControl Workflow

Goal: use the existing ChaosControl rails without importing Antithesis product assumptions or overstating evidence.

## Start

1. Read `AGENTS.md` and `README.md` in the target repository.
2. Locate the ChaosControl source from an explicit path, the current repository, or a sibling checkout.
3. Read the ChaosControl `AGENTS.md`, `README.md`, and the stage-specific documentation.
4. Select one stage from the table and read only its reference file.

| Stage | Reference |
| --- | --- |
| Property and system research | `references/research.md` |
| Rust workload onboarding | `references/workload.md` |
| VM campaign and replay promotion | `references/campaign.md` |
| Receipt-first failure triage | `references/triage.md` |

Do not store a sibling checkout path as durable product configuration. Use a pinned public source or a repository-owned input for durable integration.

## Shared constraints

- Keep the current product scope Rust-only and local-machine.
- Do not introduce Snouty, Docker Compose, Kubernetes, `agent-browser`, or Antithesis credentials.
- Obey the lifecycle system named by the target repository instructions.
- Run the relevant baseline tests before a core source change.
- Use pueue for KVM campaigns, kernel builds, and other long commands.
- Keep the functional core pure and keep process, filesystem, clock, and network work in a thin shell.
- Add positive and negative tests for each new parser, validator, assertion, workload, or evidence rule.
- Use named configuration values. Do not invent unexplained campaign counts, limits, timeouts, seeds, or buffer sizes.

## Evidence classes

Keep these classes separate:

1. A local SDK dry-run is instrumentation evidence only.
2. A bounded VM campaign is execution evidence for its recorded inputs only.
3. A replay verdict is replay evidence only when its retained artifacts pass their checks.
4. Only `snapshot_backed_reproduced` supports the selected snapshot-backed replay claim.

Raw logs are debug aids. Candidate properties, assertion counts, coverage, and passing lifecycle gates do not prove whole-system correctness.

## Stop conditions

Stop and report the exact blocker when:

- The target or ChaosControl source is ambiguous.
- Repository instructions are missing for a product mutation.
- A required lifecycle change does not exist.
- A named source artifact or receipt is missing.
- The host lacks required KVM access for a VM stage.
- An evidence class is weaker than the requested claim.
- A request needs a non-Rust SDK, a hosted service, or a container-first workflow.

## Completion report

Report:

- The target and ChaosControl source revisions.
- The exact commands that ran.
- The bounded configuration, seed, and artifact paths.
- The observed verdict and evidence class.
- The positive and negative paths that ran.
- Each blocker, gap, and non-claim.
- The next smallest action.

## Self-review

Before completion, read the selected reference again. Make sure that every claim points to a command result, receipt, or exact source artifact.
