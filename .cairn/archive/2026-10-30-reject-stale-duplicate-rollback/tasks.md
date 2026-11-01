## Phase 1: Implementation

- [x] [serial] Report an exact duplicate at an index below the replica's highest observed index as a rollback while counting the duplicate. r[chaoscontrol.smr_chain.duplicate_rollback]
- [x] [serial] Replace the stale old-index duplicate expectation with a current-index idempotent positive case and an old-index rollback negative case. r[chaoscontrol.smr_chain.duplicate_rollback]

## Phase 2: Validation and lifecycle

- [x] [serial] Run the SMR crate tests, the protocol-observation intake suite, and crate Clippy, and retain the commands and results as evidence. r[chaoscontrol.smr_chain.duplicate_rollback]
- [x] [serial] Sync the accepted requirement and archive the change. r[chaoscontrol.smr_chain.duplicate_rollback]
