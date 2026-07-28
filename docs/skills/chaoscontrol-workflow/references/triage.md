# Triage ChaosControl Evidence

Goal: explain one bounded failure from receipts and linked replay artifacts, then record a reproducible operator decision.

## Inputs

Require a replay-readiness receipt or a selected bug and replay-verdict pair. If the user gives only a raw log, locate its linked receipt before an acceptance decision.

Read `docs/operator-triage-runbook.md` and the relevant replay sections in `README.md`.

## Workflow

1. Read the readiness receipt and stable summary.
2. Select one workload, assertion ID, bug artifact, and replay verdict.
3. Confirm the replay class, reproduction status, parent depth, snapshot status, and artifact digest.
4. Cross-reference the assertion details with the target source and workload code.
5. Run the recorded reproduce command with a fresh verdict output under `target/operator-triage/`.
6. Run minimize with the same kernel, initrd, VM, and oracle inputs.
7. Compare the fresh verdict with the committed or supplied verdict.
8. Record `accepted`, `needs-refresh`, or `blocked` through the repository decision-receipt command.

Use pueue for reproduce or minimize commands that can run for a long time.

## Evidence rules

Separate observed facts from interpretation. Cite exact receipt fields, assertion details, virtual time, source paths, and artifact identities.

Use raw logs only to explain a receipt-backed failure. Do not promote a raw log, dashboard view, or assertion status into acceptance evidence.

## Negative paths

Return `blocked` when:

- The source receipt is missing or invalid.
- The bug and replay verdict do not identify the same candidate.
- The parent snapshot is missing, invalid, or has a changed digest.
- Reproduction fails.
- Minimization changes the oracle.
- A decision requires raw-log scraping.
- Evidence uses stale source or generated reports.
- The requested causal claim exceeds the observed timeline.

## Completion report

Report the selected workload, assertion ID, receipt and artifact paths, replay class, fresh reproduction result, minimized artifact, and decision receipt.

State the exact remaining blocker when the decision is not `accepted`.
