## Phase 1: Spec foundation

- [x] [serial] Write local determinism matrix and fault coverage OpenSpec artifacts.

## Phase 2: Matrix and fault models

- [x] [serial] Add local product matrix row metadata for single-machine multi-hypervisor profiles.
- [x] [parallel] Add fault coverage summary DTOs and validators for campaign receipts.
- [x] [parallel] Add negative fixtures for hidden failing rows, unsupported rows, raw-log scraping, and universal determinism/fault overclaims.

## Phase 3: Packaging and reports

- [x] [depends:matrix-fault-models] Wire selected local product matrix rows into the packaged determinism rail.
- [x] [depends:matrix-fault-models] Render fault coverage in local multi-hypervisor summaries/dashboard/readiness docs.

## Phase 4: Verification

- [x] [depends:packaging-reports] Run focused matrix/fault tests, readiness report `--check`, optional KVM matrix smoke, `openspec validate --all --strict`, and `git diff --check`.
