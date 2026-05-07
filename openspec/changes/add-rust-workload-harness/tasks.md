## Phase 1: Spec foundation

- [x] [serial] Define Rust-only workload harness scope, non-goals, and acceptance boundary.
- [x] [serial] Add delta requirements for harness surface, local dry-run reporting, downstream packaging, one-command run, and instrumentation quality reporting.
- [x] [serial] Record design decisions for harness-over-rewrite, mandatory dry-run, Nix/CLI packaging rail, and report/evidence boundaries.

## Phase 2: Harness API and template

- [x] [serial] Implement the minimal Rust workload harness/template around existing `chaoscontrol-sdk` APIs.
- [x] [depends:harness-api] Add a sample downstream-style Rust workload that uses setup/scenario conventions and existing SDK assertions/randomness.
- [x] [depends:harness-api] Document the Rust-only adoption path for the user's own projects.

## Phase 3: Local dry-run report

- [x] [depends:harness-api] Implement local dry-run execution/reporting for assertion catalog, lifecycle events, sometimes/reachable progress, and random-choice observations.
- [x] [depends:local-report] Add tests or fixtures for missing setup-complete and unexercised assertions.

## Phase 4: Packaging and bounded run rail

- [x] [depends:sample-workload] Add Nix and/or CLI helper that packages the downstream-style Rust workload as a ChaosControl guest.
- [x] [depends:packaging-rail] Add a one-command bounded workload run that writes a report path and preserves replay evidence classification boundaries.

## Phase 5: Verification and landing

- [x] [depends:run-rail] Run local dry-run verification against the sample workload and capture report evidence.
- [ ] [depends:run-rail] Run a bounded VM campaign against the sample workload and capture report/replay evidence paths.
- [x] [depends:verification] Run strict OpenSpec validation, relevant Rust/Nix checks, and `git diff --check`.
- [ ] [depends:verification] Archive the completed OpenSpec change after all implementation/evidence tasks are done.
