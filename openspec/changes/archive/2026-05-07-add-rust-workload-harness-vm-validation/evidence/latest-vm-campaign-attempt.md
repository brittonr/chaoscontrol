# VM campaign attempt summary

- status: completed
- completed_utc: 2026-05-07T18:31:31Z
- command: `timeout 3600s nix run .#explore-rust-workload -- openspec/changes/archive/2026-05-07-add-rust-workload-harness-vm-validation/evidence/vm-campaign-kcov-20260507T165920Z/run`
- transcript: `openspec/changes/archive/2026-05-07-add-rust-workload-harness-vm-validation/evidence/vm-campaign-kcov-20260507T165920Z/transcript.log.gz`
- output: `openspec/changes/archive/2026-05-07-add-rust-workload-harness-vm-validation/evidence/vm-campaign-kcov-20260507T165920Z/run`

## Result

The retry uses a KCOV-enabled kernel for the Rust workload VM rail and completed successfully with exit code 0.

Key report values from `run/report.txt`:

- Exploration rounds: 5
- Total branches explored: 20
- Unique edges found: 25
- Bugs discovered: 0
- Assertion coverage: 5/5 exercised, 5 passed, 0 failed
- Wall-clock time: 50m 43s

## Evidence boundary

`run/evidence-classification.json` records `evidence_class: bounded-vm-campaign` and explicitly states that standalone replay proof still requires replay/minimization artifacts. This satisfies VM campaign validation without promoting the bounded campaign output to snapshot-backed replay proof.

## Implementation note

The first VM attempt booted into `/init` with a non-KCOV kernel and reported `kcov: open failed (errno=2) — kernel lacks CONFIG_KCOV?`. The accepted retry is backed by `flake.nix` changing both `rust-workload-sim` and `explore-rust-workload` to use `mkChaosKernel { kcov = true; }`.
