## ADDED Requirements

### Requirement: Chunked snapshot evidence artifacts [r[replay-parent-snapshots.chunked-evidence-artifacts]]
The accepted proof coverage gate MUST accept a logical snapshot ref when either the raw `.snapshot.bin` file exists or a sidecar chunk manifest reconstructs to the same SHA-256 digest.

#### Scenario: Chunked artifact verifies [r[replay-parent-snapshots.chunked-evidence-artifacts.scenario.verifies]]
- GIVEN a replay verdict whose snapshot reference names `snapshots/<digest>.snapshot.bin`
- AND the raw snapshot file is absent
- AND `snapshots/<digest>.snapshot.bin.chunks.json` lists ordered chunk files with sizes and SHA-256 digests
- WHEN the aggregate proof coverage checker validates the workload
- THEN it MUST hash the chunk stream as the logical snapshot bytes
- AND it MUST accept the proof only if the aggregate hash matches the snapshot reference digest.

#### Scenario: Corrupt chunk rejected [r[replay-parent-snapshots.chunked-evidence-artifacts.scenario.rejects-corrupt-chunk]]
- GIVEN a chunk manifest whose listed chunk hash, size, order, or aggregate digest does not match committed bytes
- WHEN the aggregate proof coverage checker validates the workload
- THEN it MUST reject the proof before reporting accepted workload coverage.

### Requirement: Snapshot artifact size budget [r[replay-parent-snapshots.snapshot-artifact-size-budget]]
Committed snapshot evidence MUST avoid individual tracked files larger than 50 MiB unless no chunked or external artifact rail is available.

#### Scenario: Oversized artifact migrated [r[replay-parent-snapshots.snapshot-artifact-size-budget.scenario.migrated]]
- GIVEN a committed accepted-proof snapshot artifact exceeds the size budget
- WHEN the evidence is curated for repository storage
- THEN the raw oversized file SHOULD be replaced by verified chunks that each remain under the budget.
