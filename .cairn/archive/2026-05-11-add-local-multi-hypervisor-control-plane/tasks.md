## Phase 1: Spec foundation

- [x] [serial] Write local multi-hypervisor control-plane, artifact hygiene, and dashboard OpenSpec artifacts.

## Phase 2: Model and validator

- [x] [serial] Extend local campaign plan/receipt models with worker resource budgets, artifact roots, state transitions, and follow-up jobs.
- [x] [parallel] Add fail-closed fixtures for duplicate leases, missing persistence, raw-log scraping, hosted/cross-machine wording, and unbounded worker counts.
- [x] [parallel] Add artifact index model with digest/retention fields and worker/run attribution.

## Phase 3: Runner and dashboard

- [x] [depends:model-validator] Implement the local control-plane runner shell around existing replay-readiness/reproduce/minimize commands.
- [x] [depends:model-validator] Render the local multi-hypervisor dashboard from receipts/state without raw-log scraping.

## Phase 4: Verification

- [x] [depends:runner-dashboard] Run evidence model tests, dashboard rendering tests, replay-readiness report `--check`, local sample runner, optional KVM smoke, `openspec validate --all --strict`, and `git diff --check`. Evidence: `cargo test -p chaoscontrol-evidence multi_hypervisor -- --nocapture`; `cargo test -p chaoscontrol-evidence --bin replay-readiness-scheduler-receipt --no-run`; sample multi-hypervisor dashboard smoke via `cargo run -q -p chaoscontrol-evidence --bin replay-readiness-scheduler-receipt`; `cargo run -q -p chaoscontrol-evidence --bin generate-replay-readiness-report -- --check`; `openspec validate add-local-multi-hypervisor-control-plane --strict`; `openspec validate --all --strict`; `git diff --check`; `nix build .#checks.x86_64-linux.replay-readiness --no-link -L` (remote builder 10.10.10.1 unavailable, local build passed).
