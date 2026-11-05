## Why

Sparse SDK assertion emission uses a compact 32-bit alias instead of the accepted fingerprint. Distinct assertions in different namespaces may legitimately share an alias, so one can suppress the other's first pass or first failure. Stable sampling also allocates and hashes a concatenated namespace/key on each evaluation.

## What Changes

- Track sparse outcomes by accepted assertion fingerprint, shared by stable and compatibility emission paths. r[chaoscontrol.sparse_sampling.identity]
- Keep first-pass/first-fail behavior and default per-hit mode, without sampling declarations. r[chaoscontrol.sparse_sampling.outcomes]

## Impact

Current consumers: Rust workloads using `sparse_emission`, local output readers, and the VM oracle. Maintenance owner: ChaosControl SDK maintainers. Adoption: existing feature users receive the correction without API or protocol changes. Verification: fixed cross-namespace alias collision, stable/compatibility mixing, local JSONL inspection, default/all-feature/no-full Cargo checks. No new identity model, raw late declaration support, VM replay change or hosted parity claim.
