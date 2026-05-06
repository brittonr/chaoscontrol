# Design: Machine-readable replay verdicts

## Context

The current snapshot replay proof is real but scattered: `bug_*.json` contains replay parent context, snapshot refs prove durable state, the smoke script validates digests, and reproduce logs contain `BUG REPRODUCED`. That is enough for a human audit, but too implicit for automated evidence review.

## Goals

- Produce one concise JSON verdict per replay/smoke proof attempt.
- Make classification stable enough for receipts, CI summaries, and future dashboards.
- Keep Rust as the semantic source of truth for runtime verdict records while Nickel/contracts validate public evidence shapes.
- Avoid overclaiming: a snapshot-backed Raft success proves that rail, not global hypervisor determinism.

## Non-Goals

- No global deterministic-hypervisor certification in this change.
- No raw-log persistence requirement.
- No required dashboard or web server changes.

## Decisions

### 1. Rust-owned verdict artifact

**Choice:** Add a Rust-owned replay verdict record, serialized as JSON, emitted by replay-oriented commands and consumed by evidence checks.

**Rationale:** Replay outcomes depend on runtime execution, snapshot loading, digest validation, and assertion results. Rust already owns those semantics; Nickel should validate the public record shape rather than define the behavior.

**Rejected:** Let the shell smoke script remain the authority. That keeps proof logic split across logs, grep checks, and README prose.

### 2. Explicit closed classification set

**Choice:** Use a stable enum-like `replay_class` with at least:

- `snapshot_backed_reproduced`
- `snapshot_backed_not_reproduced`
- `schedule_only_replay_gap`
- `missing_snapshot_ref`
- `missing_snapshot_artifact`
- `invalid_snapshot_digest`
- `no_bug_found`
- `replay_error`

**Rationale:** Reviewers need to know whether a result is accepted proof, a known replay gap, a coverage gap, or invalid evidence without inferring from exit codes.

**Rejected:** A boolean `reproduced` field. It cannot distinguish snapshot-backed proof from schedule-only replay or invalid snapshot evidence.

### 3. Verdict binds evidence, not raw logs

**Choice:** Verdict JSON records bug artifact path, assertion id, replay parent depth, snapshot ref digest/codec/path validation status, reproduce command status, concise diagnostic, and hashes for referenced public artifacts.

**Rationale:** This preserves auditability while keeping raw logs local/ephemeral.

**Rejected:** Committing full reproduce logs as the primary proof artifact.

### 4. Smoke gate writes and validates verdict

**Choice:** The snapshot replay smoke gate should write a verdict artifact and acceptance should check the artifact fields, not only `BUG REPRODUCED` log text.

**Rationale:** This makes the proof machine-readable and keeps CI/operator behavior aligned with curated dogfood receipts.

## Risks / Trade-offs

- **Schema drift:** Mitigate with Rust tests plus Nickel/checker fixtures for positive and negative verdicts.
- **Overclaiming:** Mitigate by requiring verdict classes and docs to state that snapshot-backed Raft replay is scoped proof, not global hypervisor determinism.
- **Compatibility:** Existing dogfood evidence without verdicts should remain readable, but new accepted snapshot-backed proof should require verdict artifacts.
