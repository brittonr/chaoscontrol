## Phase 1: Spec foundation

- [x] [serial] Define assertion-readiness promotion requirements, design boundaries, and validation tasks.

## Phase 2: Gate implementation

- [x] [serial] Implement a deterministic assertion-readiness promotion checker over accepted workload proofs, assertion artifacts, and generated report output.
- [x] [serial] Add positive and negative fixtures/self-tests for preserved anti-claims, visible gap counts, and blocked overpromotion.
- [x] [serial] Wire the checker into the cheap static readiness/evidence Nix path without running VM dogfood.

## Phase 3: Evidence and closeout

- [x] [depends:Phase 2] Regenerate or check assertion-readiness status output and capture focused validation evidence.
- [x] [depends:Phase 3] Run strict OpenSpec validation, focused readiness/evidence checks, sync/archive the change, and commit the completed implementation.
