## Design

The extracted crate is a pure deterministic core. It accepts in-memory DTOs and returns validated DTOs, classifications, diagnostics, and compatibility decisions. Shell crates still read bug files, snapshot files, checkpoints, dogfood manifests, and Nickel-rendered contracts.

### Decisions

1. **One DTO authority.** `ReplayVerdict`, `ArtifactHash`, `SnapshotRef`, `ReplayParentSnapshotRef`, replay classes, and validation status values move to the shared core or are re-exported from it by compatibility modules.

2. **Public JSON compatibility first.** Existing explorer verdict output and evidence-readiness inputs keep their public field names during migration. Any field removal or rename requires a separate change.

3. **Validation is fail-closed.** The core rejects malformed digest strings, missing required snapshot references, path-escaping public artifact refs, unsupported replay classes, stale artifact hashes, and overclaim wording before a shell reports accepted proof.

4. **Nickel remains a review boundary.** Human-authored configs, receipts, and contract checks may stay Nickel-backed. Runtime bug/checkpoint/assertion/replay records remain Rust-owned DTOs validated by Rust and optionally checked by generated schemas/contracts.

5. **No VM or filesystem effects in the core.** The shared core does not read files, inspect environment, launch VMs, call clocks, query KVM, execute commands, or write receipts. It receives already-loaded values and returns decisions.

6. **Evidence claims stay bounded.** Passing validation proves DTO syntax, artifact-reference consistency, replay class admissibility, and bounded anti-claim wording. It does not prove global deterministic hypervisor behavior, downstream system correctness, or release readiness.

### Validation shape

Positive tests cover current `chaoscontrol-explore` verdict output, `chaoscontrol-evidence` accepted proof fixtures, snapshot-backed reproduced verdicts, schedule-only replay gaps, and missing-bug classifications. Negative tests cover malformed hashes, missing snapshot refs, invalid snapshot digests, path escapes, unsupported replay classes, stale artifact hashes, non-reproducing snapshot-backed bugs claimed as accepted proof, and global-determinism overclaims.
