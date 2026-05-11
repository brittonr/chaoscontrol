## Phase 1: Spec foundation

- [x] [serial] Write shared Rust simulator/VM adapter and receipt bridge OpenSpec artifacts.

## Phase 2: Adapter contracts

- [ ] [serial] Add shared Rust adapter identity/config types usable by simulator and VM harness paths.
- [ ] [parallel] Add simulator validation fixtures for unsupported environment hooks.
- [ ] [parallel] Add receipt bridge metadata and comparison helpers for simulator and VM receipts.

## Phase 3: Examples and gates

- [ ] [depends:adapter-contracts] Update at least one Rust workload example to exercise simulator and VM adapter metadata.
- [ ] [depends:receipt-bridge] Add promotion-gate tests that reject simulator-only VM replay claims.

## Phase 4: Verification

- [ ] [depends:examples-gates] Run focused SDK/evidence/simulator tests, readiness report `--check`, `openspec validate --all --strict`, and `git diff --check`.
