## Why

`validate_history` counted an exact repeat of an older replica observation as a harmless duplicate. A replica that reports command index 1 again after it has already reported index 2 has rolled back, even if the repeated observation is byte-identical to its earlier report. The protocol-observation guest intake that Molten consumes feeds real replica projections into this checker, so the escape would let a rollback pass safety.

## What Changes

- Classify an exact duplicate observation whose command index is below the replica's highest observed index as a `Rollback` safety violation.
- Keep a repeat of the replica's current highest committed index with identical facts idempotent.
- Replace the stale unit expectation that accepted an old-index duplicate with a current-index positive case and an old-index rollback negative case.

## Impact

- **Files**: `crates/chaoscontrol-smr/src/lib.rs`, `docs/smr-chain-workload.md`.
- **Testing**: `cargo test -p chaoscontrol-smr`, the protocol-observation intake suite, and Clippy for the crate.
- **Claims**: This narrows a history-validation escape. It does not establish consensus correctness or replay.
