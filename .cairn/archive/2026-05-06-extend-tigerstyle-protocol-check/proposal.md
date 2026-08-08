## Why

ChaosControl now has a focused Tigerstyle consumer gate for `chaoscontrol-fault`. The next low-noise ROI step is to widen the same proven gate to the protocol crate, which owns wire-layout constants and payload encoding used across guest and host boundaries.

## What Changes

- Extend the staged Tigerstyle consumer scope to include `chaoscontrol-protocol`.
- Keep the same small deny-list profile rather than broadening lint families.
- Preserve a single focused consumer check so CI and reviewers have one stable gate to run.

## Capabilities

### Modified Capabilities
- `local-proof-style-inputs`: The focused Tigerstyle consumer gate covers both `chaoscontrol-fault` and `chaoscontrol-protocol` owned Rust library crates.

## Impact

- **Files**: `Cargo.toml`, `flake.nix`, possible source fixes in `crates/chaoscontrol-protocol`, OpenSpec canonical spec.
- **APIs**: No public API changes are intended.
- **Dependencies**: No new dependencies; reuses pinned local Tigerstyle input.
- **Testing**: `nix build .#checks.x86_64-linux.tigerstyle-chaoscontrol-focused --no-link -L`, `nix flake check --no-build`, strict OpenSpec validation, and whitespace checks.
