## Why

The focused Tigerstyle consumer gate now covers `chaoscontrol-fault` and `chaoscontrol-protocol`. The next low-noise owned SDK surface is `chaoscontrol-sdk`, which contains guest-facing assertion, lifecycle, randomness, coverage, and transport APIs that benefit from the same staged style hardening before broader workspace rollout.

## What Changes

- Extend the focused Tigerstyle scope to include the `chaoscontrol-sdk` library crate.
- Keep the current staged lint profile and `--lib` target scope.
- Fix only SDK findings surfaced by the focused gate.

## Capabilities

### Modified Capabilities
- `local-proof-style-inputs.tigerstyle-exposure`: focused consumer coverage expands from fault+protocol to fault+protocol+SDK.

## Impact

- **Files**: `Cargo.toml`, `flake.nix`, SDK Rust files if findings require small fixes, and the local-proof/style OpenSpec.
- **APIs**: No intended public API changes.
- **Dependencies**: No new dependencies.
- **Testing**: `nix build .#checks.x86_64-linux.tigerstyle-chaoscontrol-focused --no-link -L`, strict OpenSpec validation, `nix flake check --no-build`, and whitespace checks.
