# Run a Campaign and Promote Replay Evidence

Goal: run a bounded ChaosControl VM campaign and promote only evidence that passes the selected replay contract.

## Prerequisites

- The local workload dry-run completed.
- The assertion-quality result passed or has an accepted blocker.
- The host has the required KVM access.
- The kernel, initrd, workload, fault profile, limits, and seed are explicit.

Read the campaign, replay, and readiness sections in `README.md`. Read `docs/replay-readiness-status.md` before a promotion claim.

## Workflow

1. Read the generated workload command manifest.
2. Build the exact kernel, initrd, guest, and workload artifacts.
3. Run the bounded VM campaign command from the manifest through pueue.
4. Record the run configuration, source identities, seed, assertion summary, bugs, and evidence classification.
5. If the campaign found a bug, export only the selected replay candidate.
6. Reproduce the selected bug with the recorded kernel, initrd, VM options, and parent snapshot.
7. Minimize the fault schedule without changing the selected oracle.
8. Run the relevant replay-readiness and assertion-readiness gates.

Do not add guessed defaults. If a required value is absent, stop and request or derive it from reviewed repository configuration.

## Evidence decisions

Classify results exactly:

- `instrumentation-dry-run` is not VM evidence.
- A bounded campaign is not standalone replay proof.
- A schedule-only result is gap evidence.
- A missing or invalid snapshot is blocked replay evidence.
- `snapshot_backed_reproduced` requires a valid retained parent snapshot and a successful replay verdict.

Keep simulator-local evidence separate from VMM snapshot-replay evidence.

## Negative paths

Make sure that the selected gates reject:

- Missing snapshot references.
- Missing snapshot artifacts.
- Changed artifact digests.
- Schedule-only replay.
- Non-reproducing bugs.
- Stale manifests or generated reports.
- Unsupported workload promotion.
- A universal determinism or hosted-product claim.

## Completion report

Report the pueue task IDs, exact commands, bounded configuration, evidence paths, replay class, assertion status, and failed gate names.

State that the result applies only to the selected workload, artifacts, host profile, limits, seed, faults, and observed replay path.
