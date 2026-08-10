## Why

The pinned Mantle SpaceWasm bundle now remeasures to a manifest and bundle identity that differ from the admitted ChaosControl profile. The focused Nix rail fails closed before runtime comparison, so it also blocks unrelated full-flake validation.

## What Changes

- Remeasure the immutable Mantle bundle and record the exact manifest, bundle, and runner identities.
- Refresh the typed Nickel profile, generated JSON projection, negative fixture, Rust test fixture, and operator documentation as one cohort.
- Require future identity refreshes to retain producer verification and consumer validation evidence.

## Impact

- **Files**: SpaceWasm evidence contracts, differential tests, documentation, and lifecycle evidence.
- **Testing**: Positive and negative profile checks, Rust differential tests, focused Nix differential execution, evidence contracts, and Cairn validation.
