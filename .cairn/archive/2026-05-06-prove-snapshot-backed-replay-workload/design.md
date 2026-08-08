## Context

The persisted replay parent snapshot implementation stores restorable `SimulationSnapshot` payloads behind public content-addressed refs and validates them through evidence contracts. Existing committed dogfood evidence proves the contracts, schedule-only replay gaps, and checkpoint `export-bugs` finalization, but it does not yet prove a real workload can emit a bug with nonzero replay parent depth and then reproduce from the persisted parent snapshot artifact.

## Goals / Non-Goals

**Goals:**
- Create a repeatable workload rail that can produce or import a snapshot-backed bug artifact.
- Preserve the normal evidence boundary: `bug_N.json`, snapshot artifact, receipt, hashes, and concise reproduce/minimize status.
- Make no-bug or schedule-only outcomes explicit negative/coverage-gap evidence instead of silently passing.

**Non-Goals:**
- Guarantee that the current Raft `fig8_commit` target is the only acceptable workload.
- Commit raw logs or huge checkpoints as proof.
- Promote redb to a public evidence format.

## Decisions

### 1. Workload evidence is distinct from fixture proof

**Choice:** Acceptance requires at least one curated workload artifact whose bug evidence has `replay_parent_depth > 0` and a valid `replay_parent_snapshot_ref`, or an explicitly recorded negative result explaining why the attempted workload did not reach that state.

**Rationale:** Unit tests and fixtures prove mechanics, but they do not exercise the operator path that captures, exports, receipts, and replays real campaign artifacts.

**Alternative:** Treat existing contract fixtures as sufficient. Rejected because fixtures can pass while workload scheduling/finalization never persists refs in practice.

**Implementation:** Add a targeted campaign/export/reproduce recipe, tune or add a small workload if Raft cannot reliably produce parent-context bugs, and record concise evidence under `dogfood-results/<run>/`.

### 2. Receipts classify replay outcomes

**Choice:** Receipts must classify outcomes as snapshot-backed replay success, snapshot-backed replay failure/gap, schedule-only replay gap, no-bug campaign, or skipped due to missing required artifact.

**Rationale:** Prior evidence mixed schedule-only replay failures with broader replay evidence. Outcome classification prevents overclaiming.

**Alternative:** Store only command exit statuses. Rejected because exit status alone does not say whether the snapshot-backed path was exercised.

**Implementation:** Extend receipt text/materialization or checker logic as needed so artifact hashes bind the classification and snapshot refs.

### 3. Export remains the finalization path for interrupted campaigns

**Choice:** If a campaign times out or is interrupted after checkpoint-held bugs exist, `chaoscontrol-explore export-bugs` is the supported finalization path.

**Rationale:** This avoids ad hoc resume/killing and preserves snapshot-ref validation semantics.

**Alternative:** Continue forcing finalization by resuming and killing when `bug_N.json` appears. Rejected as brittle and not operator-grade.

**Implementation:** The workload rail should use `export-bugs --checkpoint ... --output ...` for interrupted runs and record whether exported bugs retained required snapshot refs.

## Risks / Trade-offs

**Workload may not naturally produce nonzero parent-depth bugs** → Mitigate by allowing a focused deterministic workload or tuned scenario as long as it uses the real explorer/export/reproduce path and not fixture-only serialization.

**Evidence size can grow quickly** → Mitigate with summaries, hashes, selected bug artifacts, and raw-log exclusion.

**Reproduce may still fail** → Treat this as valid negative replay-gap evidence if the bug has a valid snapshot ref and the receipt clearly states the failure.
