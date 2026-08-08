## Why

The coverage bitmap saturates after round 1-2 because AFL-style code edges don't distinguish protocol states. Two branches that execute the same code paths but with different term numbers, leader counts, or commit indices look identical to the explorer. The Raft guest works around this with hand-coded `coverage::record_edge(9000 + alive_mask * 10 + partition_count)` calls — fragile, per-guest boilerplate that shouldn't exist. The SDK already has `send_event` for structured protocol state, and assertion details carry JSON with term/role/index data. The explorer ignores all of it.

## What Changes

- **Protocol state hashing in the SDK**: New `coverage::record_state()` call that takes a set of key-value pairs and hashes them into coverage edges automatically. Replaces the manual `record_edge(MAGIC + field1 * K + field2)` pattern.
- **Event-driven coverage enrichment**: The explorer hashes `OracleEvent` names and details into coverage edges after each branch, so branches that produce different event sequences (leader elected in term 3 vs term 5) look distinct.
- **Assertion-detail coverage enrichment**: The explorer hashes the JSON details from assertion hits (not just verdict/hit-count) into coverage, so "election_safety with term=3, leader=1" vs "term=5, leader=2" are different edges.
- **Raft guest migration**: Replace hand-coded `record_edge(MAGIC + ...)` calls with `record_state` calls for cleaner protocol-state tracking.

## Capabilities

### New Capabilities
- `protocol-state-coverage`: SDK `record_state` API, explorer-side event/detail enrichment into coverage bitmap, and migration of manual edge hacks.

### Modified Capabilities
_(none)_

## Impact

- **chaoscontrol-sdk**: New `coverage::record_state(&[("key", "value")])` function. Hashes key-value pairs into coverage edges using the existing bitmap.
- **chaoscontrol-explore**: `enrich_with_protocol_state` function alongside existing `enrich_with_assertion_state` and `enrich_with_schedule_fingerprint`. Uses `OracleReport.events` and assertion detail JSON.
- **chaoscontrol-raft-guest**: Replace ~10 hand-coded `record_edge(MAGIC + ...)` lines with `record_state` calls. Remove magic number constants.
- **Coverage bitmap**: Protocol-state edges share the code region `[0, CODE_REGION_END)` since they flow through the same guest-side bitmap. Explorer-side enrichment from events/details uses the assertion region `[CODE_REGION_END, ASSERTION_REGION_END)`.
