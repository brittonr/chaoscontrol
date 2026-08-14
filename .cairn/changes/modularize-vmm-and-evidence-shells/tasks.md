## Tasks

- [x] [serial] Define the VMM, controller, evidence, unsafe-owner, and dependency-direction module map. r[chaoscontrol.architecture_modules.ownership] r[chaoscontrol.architecture_modules.boundary]
- [ ] [depends:architecture-module-map] Record focused behavior, API, schema, Clippy, and unsafe-path baselines before core movement. r[chaoscontrol.architecture_modules.migration]
- [ ] [depends:architecture-baseline] Extract pure VM construction, transition, snapshot, poison, and teardown plans from `vm.rs`. r[chaoscontrol.architecture_modules.vmm]
- [ ] [depends:architecture-baseline] Extract pure scheduling, fault, observation, and multi-VM transition plans from `controller.rs`. r[chaoscontrol.architecture_modules.controller]
- [ ] [depends:architecture-baseline] Split evidence loading, classification, orchestration, rendering, and publication ownership. r[chaoscontrol.architecture_modules.evidence]
- [ ] [parallel] Add dependency-direction validation and negative fixtures for forbidden shell effects in cores. r[chaoscontrol.architecture_modules.boundary] r[chaoscontrol.architecture_modules.validation]
- [ ] [parallel] Add focused unsafe ownership, timer, handle-lifetime, cancellation, poison, and teardown tests. r[chaoscontrol.architecture_modules.validation]
- [ ] [depends:architecture-callsite-migration] Remove temporary compatibility re-exports after all call sites use the owned modules. r[chaoscontrol.architecture_modules.migration]
- [ ] [depends:architecture-validation] Run focused, workspace, schema, Cairn, and relevant Nix validation after each slice and at closeout. r[chaoscontrol.architecture_modules.validation]
