## Phase 1: Spec Foundation

- [x] [serial] Define the Rust-owned evidence/readiness migration boundary, compatibility rules, and first implementation slice.

## Phase 2: Rust Evidence Core

- [x] [serial] Add a Rust evidence/readiness core crate or module with typed models for accepted workload proofs, replay verdicts, assertion summaries, snapshot refs, and chunk manifests.
- [x] [depends:core-models] Port aggregate replay proof coverage validation into Rust with positive committed-evidence tests and negative malformed/tampered fixture tests.
- [x] [depends:core-models] Add Rust generation/check support for `docs/replay-proof-coverage.md` so the public coverage doc derives from `dogfood-results/accepted-workload-proofs.json`.
- [x] [parallel] Port snapshot chunk materialization validation into Rust, including missing, reordered, corrupt, unsafe path, wrong digest, and temp-cleanup negative tests.

## Phase 3: Readiness Reports and Gates

- [x] [depends:core-models] Port replay readiness report generation/checking into Rust while preserving current Markdown and anti-claim output.
- [ ] [depends:core-models] Port assertion readiness report generation/checking into Rust while preserving current Markdown and promotion guidance output.
- [ ] [depends:readiness-reports] Port readiness promotion, assertion promotion, surface drift, dogfood artifact size, SDK local report track, and accepted dogfood config checks into Rust or explicitly document any retained non-policy wrapper boundary.

## Phase 4: Operator/Nix Integration

- [ ] [depends:replay-proof-rust] Wire the replay proof coverage Rust CLI and generated coverage doc check into Nix evidence/readiness checks before removing the Python gate from those checks.
- [ ] [depends:readiness-rust] Wire migrated Rust readiness/report CLIs into `nix run .#replay-readiness`, CI artifacts, README documentation, and the full flake check.
- [ ] [depends:nix-wiring] Remove migrated Python/Bash proof-policy scripts only after Rust parity, negative tests, and Nix checks pass.

## Phase 5: Verification

- [ ] [depends:nix-wiring] Run focused Rust tests, focused Nix evidence/readiness checks, OpenSpec strict validation, cargo-audit/cargo-deny gates, and final `nix flake check -L` before archive.
