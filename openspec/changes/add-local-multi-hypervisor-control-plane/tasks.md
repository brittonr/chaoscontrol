## Phase 1: Spec foundation

- [x] [serial] Write local multi-hypervisor control-plane, artifact hygiene, and dashboard OpenSpec artifacts.

## Phase 2: Model and validator

- [ ] [serial] Extend local campaign plan/receipt models with worker resource budgets, artifact roots, state transitions, and follow-up jobs.
- [ ] [parallel] Add fail-closed fixtures for duplicate leases, missing persistence, raw-log scraping, hosted/cross-machine wording, and unbounded worker counts.
- [ ] [parallel] Add artifact index model with digest/retention fields and worker/run attribution.

## Phase 3: Runner and dashboard

- [ ] [depends:model-validator] Implement the local control-plane runner shell around existing replay-readiness/reproduce/minimize commands.
- [ ] [depends:model-validator] Render the local multi-hypervisor dashboard from receipts/state without raw-log scraping.

## Phase 4: Verification

- [ ] [depends:runner-dashboard] Run evidence model tests, dashboard rendering tests, replay-readiness report `--check`, local sample runner, optional KVM smoke, `openspec validate --all --strict`, and `git diff --check`.
