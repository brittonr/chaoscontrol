## Why

The Raft dogfood replay gap showed that a fault schedule alone is not enough to reproduce every discovered branch. ChaosControl now records and consumes the parent snapshot in memory, but standalone replay still needs a durable way to find, verify, and restore the exact parent snapshot context after the run has ended.

Without a bounded persisted snapshot artifact boundary, bug reports either become huge opaque JSON blobs or remain schedule-only evidence that reviewers cannot replay independently.

## What Changes

- **Snapshot artifact references**: Bug and receipt evidence will reference parent snapshots by content-addressed IDs instead of embedding raw snapshot payloads directly in public evidence JSON.
- **Snapshot store abstraction**: Replay will load snapshots through a Rust-owned `SnapshotStore` interface with a transparent file-backed layout and an optional host-side redb index/store implementation.
- **Evidence contracts**: Nickel/Rust validation will check snapshot references, hashes, codec metadata, replay context, and negative cases such as missing, corrupt, or wrong-hash snapshots.
- **Replay CLI behavior**: Standalone replay will fail early with actionable diagnostics when a required parent snapshot cannot be loaded, and will use the persisted snapshot when it is present.

## Capabilities

### New Capabilities
- `replay-parent-snapshots`: Persist, reference, validate, and load parent snapshots required for deterministic standalone replay.

### Affected Existing Surfaces
- `nickel-evidence-contracts`: Receipts and bug records include validated snapshot references while preserving Rust ownership of runtime snapshot bytes.
- `auto-minimize`: Minimization and replay use the same persisted parent snapshot boundary when replaying saved bug reports.

## Impact

- **Files**: `crates/chaoscontrol-explore`, `crates/chaoscontrol-replay`, `crates/chaoscontrol-vmm` serialization surfaces, `contracts/evidence`, `scripts/check-evidence-contracts.py`, dogfood receipt materialization, README/operator docs.
- **APIs**: New Rust snapshot store trait and public bug/receipt reference fields; no change to guest workload APIs.
- **Dependencies**: redb may be added to a host-side evidence-store crate/feature only if the implementation chooses the optional indexed store; file-backed store remains the baseline.
- **Testing**: Unit tests for snapshot store round trips and corruption checks, contract fixtures for valid/invalid refs, and a dogfood replay fixture proving a saved bug can restore from its parent snapshot artifact.
