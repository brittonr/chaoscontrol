## Phase 1: Chunked artifact contract

- [x] [serial] Add a chunk-manifest fallback to the replay proof coverage checker.
- [x] [depends:checker] Document chunked snapshot evidence and materialization expectations.

## Phase 2: Evidence migration

- [x] [depends:checker] Convert oversized committed snapshot artifacts to chunk manifests and sub-50MB parts.
- [x] [depends:migration] Refresh affected artifact hashes and aggregate proof docs if needed.

## Phase 3: Validation and closeout

- [x] [depends:migration] Run proof/evidence/readiness/whitespace gates and a tracked-file size guard.
- [x] [depends:validation] Archive this OpenSpec, commit, push, and verify clean sync.
