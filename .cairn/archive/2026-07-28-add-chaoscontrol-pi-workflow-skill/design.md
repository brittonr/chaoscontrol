## Context

ChaosControl already owns the Rust SDK, deterministic campaign engine, replay tools, evidence models, and local triage surfaces. The Pi installation has no ChaosControl-specific skill that connects these surfaces into one bounded workflow.

The workflow must preserve repository-local lifecycle rules. It must also keep instrumentation, VM execution, and snapshot-backed replay as different evidence classes.

## Goals / Non-Goals

**Goals:**

- Give Pi one discoverable entry point for ChaosControl work.
- Load detailed stage instructions only when the selected task requires them.
- Use existing ChaosControl commands and evidence artifacts as the authority.
- Preserve positive and negative paths, claim boundaries, and explicit stop conditions.
- Keep the source in the repository and install it through a global symbolic link.

**Non-Goals:**

- Add Antithesis, Snouty, Docker, Kubernetes, or browser automation.
- Change the ChaosControl SDK, runtime, schemas, or supported product scope.
- Treat candidate properties, dry-run output, or campaign output as replay proof.
- Replace Cairn, OpenSpec, portfolio search, pueue, or repository-local instructions.

## Decisions

### Use one skill with focused references

The source will contain one compact `SKILL.md`. Separate reference files will own research, workload, campaign, and triage procedures.

This design keeps the always-loaded skill description small. It also prevents a triage request from loading workload-onboarding details.

A group of independent skills was rejected. The stages share discovery, lifecycle, evidence, and claim-boundary rules that need one source of truth.

### Keep the repository copy authoritative

The canonical source will live at `docs/skills/chaoscontrol-workflow/`. The global Pi directory will contain a symbolic link to this source.

Copying the skill into the home directory was rejected. A copied skill can drift from the commands and evidence boundaries in the repository.

### Delegate decisions to existing authorities

The skill will delegate lifecycle work to the target repository rules. It will use portfolio search for uncertain property research and pueue for long campaigns.

ChaosControl commands will produce runtime evidence. Cairn, OpenSpec, Valence, and repository gates will keep their existing authority boundaries.

### Make evidence classes explicit

The workflow will label local SDK output as instrumentation evidence. It will label bounded campaigns as VM execution evidence.

Only a valid `snapshot_backed_reproduced` verdict can support the selected snapshot-backed replay claim. The skill will not promote raw logs or candidate properties.

## Risks / Trade-offs

- **Skill guidance can drift from commands** → Link the skill to canonical repository docs and include a focused source audit.
- **A broad trigger can load the skill too often** → Use a specific description that names ChaosControl tasks and artifacts.
- **Agents can overstate campaign results** → Put evidence classes and non-claims in the main skill and each stage reference.
- **The global link depends on this checkout path** → Keep installation local and report the resolved source path. Remove the link for rollback.
