## Why

ChaosControl has many focused examples and negative tests, but only limited generated sequence testing. Stateful scheduler, snapshot, fault, assertion, virtio, and evidence logic can fail through transition combinations that hand-written cases miss.

## What Changes

- Add bounded model-based property tests for selected pure cores.
- Generate valid and invalid transition sequences with deterministic seeds.
- Compare implementation state with small reference models.
- Preserve minimized counterexamples as stable regression fixtures.
- Keep KVM behavior tests separate from pure state-machine properties.

## Impact

- **Code**: test-only generators, reference models, invariant oracles, and regression fixtures.
- **Targets**: scheduler, snapshots, fault ledger, assertion identity, virtio transport, and evidence validation.
- **CI**: a fast deterministic property lane plus a separately scheduled deeper lane.

## Non-Goals

- No formal proof claim.
- No unbounded fuzzing in normal CI.
- No replacement for KVM, integration, or proof-tool validation.
