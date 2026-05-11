## Phase 1: Spec foundation

- [x] [serial] Write shared Rust simulator/VM adapter and receipt bridge OpenSpec artifacts.

## Phase 2: Adapter contracts

- [x] [serial] Add shared Rust adapter identity/config types usable by simulator and VM harness paths.
- [x] [parallel] Add simulator validation fixtures for unsupported environment hooks.
- [x] [parallel] Add receipt bridge metadata and comparison helpers for simulator and VM receipts.

## Phase 3: Examples and gates

- [x] [depends:adapter-contracts] Update at least one Rust workload example to exercise simulator and VM adapter metadata.
- [x] [depends:receipt-bridge] Add promotion-gate tests that reject simulator-only VM replay claims.

## Phase 4: Verification

- [x] [depends:examples-gates] Run focused SDK/evidence/simulator tests, readiness report `--check`, `openspec validate --all --strict`, and `git diff --check`.
