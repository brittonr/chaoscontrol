## Context

Snapshot refs are Rust-derived and intentionally point at `snapshots/<sha256>.snapshot.bin`; that path is also embedded in committed bug/verdict evidence. Rewriting those refs would invalidate the evidence boundary. The storage optimization therefore sits at the committed artifact layer: keep the logical snapshot path stable, but allow the repository to carry chunked bytes plus a manifest when the raw file would be too large.

## Decisions

### 1. Chunk manifests are sidecars

**Choice:** Store `<snapshot>.chunks.json` next to `<snapshot>.partNN` files.

**Rationale:** The original ref path remains stable for bug/verdict JSON. The aggregate checker can reconstruct and hash the logical bytes without requiring the raw large file in Git.

**Rejected:** Changing the snapshot ref schema to point directly at chunks. That would mix runtime replay refs with repository storage mechanics and require Rust/runtime changes for an evidence-only problem.

### 2. Checker verifies aggregate bytes without materializing by default

**Choice:** `scripts/check-replay-proof-coverage.py` streams chunk bytes into SHA-256 and validates per-chunk hashes/sizes.

**Rationale:** CI and local checks stay lightweight and do not need to write large reconstructed files.

### 3. Raw files remain valid for small artifacts

**Choice:** The checker accepts raw `.snapshot.bin` files when present and chunk manifests when raw files are absent.

**Rationale:** redb and other small proofs do not need needless chunking.

## Risks / Trade-offs

- Chunking reduces per-file GitHub warnings, not total evidence bytes. A future artifact-store/LFS rail may still be better.
- Manual replay from committed evidence now requires reconstructing chunked snapshots first if the raw file is absent; docs must make this explicit.
