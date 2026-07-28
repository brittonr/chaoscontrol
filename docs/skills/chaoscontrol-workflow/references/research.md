# Research a ChaosControl Workload

Goal: produce a review-needed property portfolio with exact source evidence and no automatic correctness claims.

## Inputs

Identify the target repository, subsystem, and relevant external references. Treat documentation, issues, and incident notes as leads until source or runtime evidence supports them.

Use `portfolio-search` when several failure mechanisms or property shapes are plausible.

## Workflow

1. Read the target architecture, lifecycle artifacts, source, and tests.
2. Inventory existing ChaosControl SDK imports and assertion calls.
3. Map state, concurrency, persistence, authority, clocks, randomness, and external effects.
4. Identify failure-prone transitions and partial-failure boundaries.
5. Write candidate safety, liveness, reachability, and unreachable-path properties.
6. Evaluate the candidates for duplication, dominance, observability, and missing fault classes.

Use syntax-aware search for Rust assertion calls when practical. Record each existing assertion type, message, category, source path, and line.

## Candidate property record

Give each candidate a stable kebab-case ID and these fields:

- Property statement.
- Safety, liveness, reachability, or unreachable-path class.
- Proposed ChaosControl assertion type.
- External-harness or in-process observation source.
- Source paths and relevant state transitions.
- Applicable fault classes.
- Boundary and configured-limit value families.
- Expected passing and failing observations.
- Open questions and investigation log.
- Evidence class needed for promotion.
- Explicit non-claims.

## Output

Use the artifact root selected by the target repository. If no root exists, use `target/chaoscontrol-research/` for scratch output.

Produce:

- `system-analysis.md`
- `existing-assertions.md`
- `property-catalog.md`
- `property-relationships.md`
- `evaluation.md`

Scratch output is review-needed evidence. Do not copy it into accepted Cairn or OpenSpec requirements without review.

## Negative paths

Reject or mark a candidate when:

- Its condition cannot be observed.
- Its oracle restates the implementation.
- Its missing proof step is equivalent to the original claim.
- It duplicates a stronger property.
- It depends on unsupported faults or product scope.
- It treats an issue report as proof.
- It promotes coverage, reachability, or a local dry-run into replay evidence.

## Completion evidence

Make sure that every candidate has a source path, an assertion rationale, an expected failure observation, and a visible unresolved-question list.
