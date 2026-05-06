# Tasks

## 1. Spec

- [x] Add replay-verdict requirements for first-class machine-readable replay classifications.
- [x] Validate the OpenSpec change strictly before implementation.

## 2. Implementation

- [ ] Add a Rust-owned replay verdict model and JSON serialization for replay/smoke proof attempts.
- [ ] Emit verdict artifacts from standalone reproduce or the replay proof wrapper used by the smoke gate.
- [ ] Update the snapshot replay smoke gate to validate verdict fields instead of relying only on log-text matching.
- [ ] Add evidence contract/checker coverage for positive and negative replay verdict fixtures.
- [ ] Document replay verdict classes and the scoped distinction between proven snapshot-backed replay and broader hypervisor determinism.

## 3. Verification

- [ ] Run Rust tests covering verdict serialization and classification.
- [ ] Run evidence contract and registry checks.
- [ ] Run the KVM snapshot replay smoke check and confirm it writes an accepted verdict artifact.
- [ ] Archive this OpenSpec after implementation evidence is complete.
