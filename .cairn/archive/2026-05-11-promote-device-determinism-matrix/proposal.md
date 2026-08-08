## Why

ChaosControl currently has bounded hide-TSC VM drift evidence, but item 5 in the competitor gap list remains open: we do not have broad arbitrary guest/device determinism proof comparable to mature deterministic simulation platforms. Operators need a promotion path that expands the existing drift gate without allowing a single profile-specific receipt to become a universal determinism claim.

## What Changes

- Add a device/profile determinism matrix over the existing VM drift gate.
- Require positive and negative matrix evidence before any stronger determinism support label is emitted.
- Keep the current bounded hide-TSC rail as the default; broader profiles remain unpromoted until matrix evidence exists.

## Capabilities

### Modified Capabilities
- `vm-determinism-drift`: adds matrix planning, per-profile receipts, negative drift fixtures, and anti-overclaim promotion checks.

## Impact

- **Files**: `crates/chaoscontrol-evidence`, `crates/chaoscontrol-vmm` or determinism stress runners, Nix checks, `docs/replay-readiness-status.md`, `openspec/specs/vm-determinism-drift/spec.md`.
- **APIs**: likely adds matrix config/receipt structs and CLI flags for profile selection.
- **Testing**: pure matrix aggregation tests, negative fixture tests, focused CLI smoke, and packaged Nix drift gate.
