## Phase 1: Spec foundation

- [x] [serial] Write scaffold/promotion and assertion-quality OpenSpec artifacts.

## Phase 2: Rust scaffold and checks

- [x] [serial] Add the Rust workload scaffold/template command or Nix app with explicit generated inputs and commands.
- [x] [parallel] Add assertion quality gate logic over local SDK reports with positive and negative fixtures.
- [x] [parallel] Update Rust workload template docs to show local dry-run, VM campaign, and promotion checklist.

## Phase 3: Packaging and verification

- [x] [depends:scaffold] Package the scaffolded sample in the local readiness/check rail.
- [x] [depends:quality-gate] Run SDK/evidence focused tests, scaffold sample dry-run, `openspec validate --all --strict`, and `git diff --check`.
