## Phase 1: Specification foundation

- [x] [serial] Define the Rust workload snapshot replay proof scope, requirements, design, and verification plan.

## Phase 2: Probe implementation

- [x] [serial] Add opt-in Rust workload snapshot probe cmdline parsing, explicit assertion ID, and tests.
- [ ] [depends:probe] Add a Rust workload accepted-verdict dogfood command or documented invocation using the existing wrapper.

## Phase 3: Evidence

- [ ] [depends:dogfood] Run the Rust workload accepted snapshot verdict dogfood rail and curate concise evidence.
- [ ] [depends:evidence] Update replay proof coverage/readiness manifests for the Rust workload proof.
- [ ] [depends:verification] Run strict OpenSpec validation, replay proof coverage checks, focused Rust/Nix checks, and `git diff --check`.
