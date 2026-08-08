## Phase 1: Specification

- [x] Create the OpenSpec proposal, design, tasks, and local-proof/style delta for adding SDK coverage.

## Phase 2: Implementation

- [x] Add `chaoscontrol-sdk` to the focused Tigerstyle workspace metadata and root flake package list.
- [x] Fix any SDK findings required for the staged Tigerstyle profile without changing public semantics.

## Phase 3: Verification

- [x] Run `./scripts/openspec validate extend-tigerstyle-sdk-check --strict`.
- [x] Run `nix build .#checks.x86_64-linux.tigerstyle-chaoscontrol-focused --no-link -L`.
- [x] Run `nix flake check --no-build`.
- [x] Run `git diff --check` before archiving and `git diff --cached --check` before commit.
