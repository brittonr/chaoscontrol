## Phase 1: Scope and Spec

- [x] Inspect current workspace members and existing Tigerstyle check scope.
- [x] Create OpenSpec change for full-workspace staged Tigerstyle rollout.
- [x] Validate the OpenSpec change strictly.

## Phase 2: Implementation

- [x] Update Cargo workspace Tigerstyle metadata to include every workspace package.
- [x] Update the Nix Tigerstyle consumer check package list to include every workspace package.
- [x] Fix source findings surfaced by the staged Tigerstyle profile without broad suppressions.

## Phase 3: Verification and Archive

- [x] Run the full-workspace Tigerstyle consumer check successfully.
- [x] Run `nix flake check --no-build` successfully.
- [x] Run OpenSpec validation and whitespace checks successfully.
- [x] Archive the completed OpenSpec change and validate the canonical spec.
