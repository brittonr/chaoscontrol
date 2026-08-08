## Phase 1: Spec and provenance

- [x] Create an OpenSpec package describing local Tigerstyle and verified-logic input adoption.
- [x] Pin `../tigerstyle` and `../verified-logic` sibling HEAD revisions in `flake.lock`.

## Phase 2: Flake wiring

- [x] Expose the pinned verified-logic package in ChaosControl packages and the default dev shell.
- [x] Add a ChaosControl flake check for the pinned verified-logic Verus proof rail.
- [x] Expose pinned Tigerstyle cargo/standards packages and the policy-registry check.

## Phase 3: Verification

- [x] Run `nix flake check --no-build`.
- [x] Run `nix build .#checks.x86_64-linux.tigerstyle-policy-registry --no-link -L`.
- [x] Run `nix build .#checks.x86_64-linux.verified-logic-verus-proofs --no-link -L`.
