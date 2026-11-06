## Why

An agent should be able to locate a recorded failure, open it in a fresh process,
inspect it on a disposable branch, and validate a candidate build without a human
relaying debugger output. ChaosControl already streams `BugFound` mid-run and can
export checkpoint-held bugs, but the event carries no stable failure identity,
snapshot reference, or artifact path, so a subscriber cannot start an investigation
until the campaign ends. GitHub issue #1 records the full phased contract; this
change admits it natively and implements the first, reference-only slice.

[Antithesis, "Your agent can now ship reliable distributed systems"](https://antithesis.com/blog/2026/time-traveling-agents/)
and [its debugging documentation](https://antithesis.com/docs/product/debugging/simple_mvd/)
are comparison material. This change does not import Snouty, vendor credentials,
hosted services, container-runtime assumptions, or claim Antithesis parity.

## What Changes

Add a versioned owner for the local agent investigation contract as a native Cairn
specification, and implement its first bounded slice: every mid-run `BugFound`
event carries the exact failure dedup key and the replay parent depth, and
`export-bugs` can select checkpoint-held bugs by that dedup key. A subscriber can
therefore correlate a live event with the checkpoint record and materialize the
matching artifact without waiting for the normal end-of-run writer.

The remaining phases (session admission, moment addressing, branch creation,
guest diagnostic operations, intervention recording, candidate validation and
receipts) stay specified but unimplemented; their tasks remain unchecked.

## Consumer and Ownership

Consumer: agent-driven triage that watches the dashboard stream and starts
read-only investigation from the first observed failure. Durable capability:
stable mid-run failure identity that composes with existing checkpoint export.
Adoption: SSE/dashboard consumers read the new event fields; `export-bugs` gains
`--dedup-key`; existing events and filters remain valid. Maintenance owner:
chaoscontrol-explore and chaoscontrol-replay maintainers. Repeatability: permanent
event-serialization and checkpoint-filter regressions plus the existing export CLI
tests.

## Non-Goals

No branch creation, guest diagnostic execution, session admission, intervention
persistence, candidate-build validation, receipt format, MCP/HTTP adapter,
autonomous merge or deploy, cross-machine scheduling, host/guest shell, new
snapshot store, or redefinition of replay/witness verdict classes. A matching
dedup key does not establish that two failures are the same event stream or that a
snapshot is replayable; it is a correlation field only.
