## Phase 1: Promotion wiring

- [x] [serial] Add the OpenSpec proposal, design, tasks, and replay-readiness operator delta for bounded local control-plane promotion.
- [x] [serial] Reclassify the generated local multi-hypervisor control-plane status as supported bounded-local evidence.
- [x] [depends:status] Add promotion-gate checks and negative tests for missing local-control-plane evidence tokens and hosted/cross-machine overclaims.
- [x] [depends:status] Regenerate README/status docs so the roadmap no longer lists local control-plane depth as a current missing feature.

## Phase 2: Verification and archive

- [x] [depends:promotion-gate] Run focused evidence tests and generated report checks.
- [x] [depends:focused-tests] Run the replay-readiness Nix rail and OpenSpec strict validation.
- [x] [depends:verification] Archive the completed OpenSpec and commit the promotion.
