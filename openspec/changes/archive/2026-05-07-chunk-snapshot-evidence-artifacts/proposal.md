## Why

Accepted workload proofs require committed snapshot bytes so the aggregate proof checker can verify the Rust-owned replay verdict digest. The new networking proof exposed that raw `.snapshot.bin` artifacts can exceed GitHub's recommended per-file size and make the source tree heavy.

## What Changes

- **Chunk manifests**: Large committed snapshot artifacts may be stored as deterministic chunk files plus a JSON manifest keyed by the original `.snapshot.bin` path.
- **Coverage checker**: The proof coverage gate verifies either a raw snapshot artifact or its chunk manifest, including every chunk hash and the reconstructed aggregate SHA-256 digest.
- **Evidence curation**: Existing oversized accepted-proof snapshots are converted to chunked evidence; manifest/readiness paths continue to name the original snapshot ref path so replay verdicts remain unchanged.

## Verification

Run the aggregate proof/evidence gates and ensure no tracked snapshot chunk exceeds the configured size budget.
