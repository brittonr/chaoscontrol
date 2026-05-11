## Why

Single-machine multi-hypervisor campaigns become more valuable as local determinism and fault coverage deepen. Current determinism matrix evidence is bounded, and fault models exist, but the local product needs an explicit path to expand selected device/profile rows and richer deterministic fault scenarios without claiming universal determinism.

## What Changes

- **Local determinism matrix expansion**: Add selected rows for the local multi-hypervisor product profile.
- **Fault coverage receipts**: Record which network/block/timer/process fault classes a local campaign exercised.
- **Negative evidence**: Preserve failing/unsupported rows as explicit bounded evidence rather than hiding them.

## Capabilities

### Modified Capabilities
- `vm-determinism-drift`: Expands bounded local device/profile matrix requirements.
- `replay-readiness-operator`: Adds local fault coverage summaries to campaign readiness artifacts.

## Impact

- **Files**: Determinism matrix models/runners, fault coverage summaries, readiness docs/gates.
- **APIs**: Receipt schema additions; no SDK language changes.
- **Dependencies**: None expected.
- **Testing**: Pure matrix/fault summary tests, negative fixtures, optional KVM matrix smoke.
