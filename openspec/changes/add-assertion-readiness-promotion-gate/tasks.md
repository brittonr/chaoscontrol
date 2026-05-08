## Phase 1: Spec foundation

- [x] [serial] Define assertion-readiness promotion requirements, design boundaries, and validation tasks.

## Phase 2: Gate implementation

- [ ] [serial] Implement a deterministic assertion-readiness promotion checker over accepted workload proofs, assertion artifacts, and generated report output.
- [ ] [serial] Add positive and negative fixtures/self-tests for preserved anti-claims, visible gap counts, and blocked overpromotion.
- [ ] [serial] Wire the checker into the cheap static readiness/evidence Nix path without running VM dogfood.

## Phase 3: Evidence and closeout

- [ ] [depends:Phase 2] Regenerate or check assertion-readiness status output and capture focused validation evidence.
- [ ] [depends:Phase 3] Run strict OpenSpec validation, focused readiness/evidence checks, sync/archive the change, and commit the completed implementation.
