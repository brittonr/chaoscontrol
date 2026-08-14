## Tasks

- [x] [serial] Define target cores, model boundaries, invariant classes, deterministic profiles, and non-claims. r[chaoscontrol.property_coverage.profile] r[chaoscontrol.property_coverage.boundary]
- [ ] [depends:property-foundation] Add shared seeded sequence, invalid-command, invariant, shrink, and counterexample support. r[chaoscontrol.property_coverage.framework]
- [ ] [depends:property-framework] Add scheduler and snapshot reference models with valid and invalid sequence generators. r[chaoscontrol.property_coverage.scheduler_snapshot]
- [ ] [depends:property-framework] Add fault-ledger and assertion-identity reference models. r[chaoscontrol.property_coverage.fault_assertion]
- [ ] [depends:property-framework] Add virtio transport and evidence-admission reference models. r[chaoscontrol.property_coverage.virtio_evidence]
- [ ] [parallel] Add no-mutation, exact-commit, capacity, continuation, binding, replay, and deterministic-result invariants. r[chaoscontrol.property_coverage.invariants]
- [ ] [depends:property-targets] Preserve minimized failures as stable positive and negative regression fixtures. r[chaoscontrol.property_coverage.shrink]
- [ ] [depends:property-regressions] Add bounded fast and deep lanes with recorded seeds and named limits. r[chaoscontrol.property_coverage.ci]
- [ ] [depends:property-validation] Run focused properties, regression fixtures, workspace, Cairn, and relevant Nix validation. r[chaoscontrol.property_coverage.validation]
