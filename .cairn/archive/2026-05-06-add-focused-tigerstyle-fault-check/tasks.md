## Phase 1: Specification

- [x] Create the focused Tigerstyle consumer-check OpenSpec.

## Phase 2: Implementation

- [x] Add the staged Tigerstyle lint profile.
- [x] Add the root flake consumer check for `chaoscontrol-fault`.

## Phase 3: Verification

- [x] Run `nix build .#checks.x86_64-linux.tigerstyle-chaoscontrol-fault --no-link -L`.
- [x] Run `nix flake check --no-build`.
- [x] Validate OpenSpec and whitespace.
