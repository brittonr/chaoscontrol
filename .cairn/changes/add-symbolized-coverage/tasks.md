# Tasks

## Phase 1: Baseline and catalog format

- [x] [serial] Record current SanCov, KCOV, coverage bitmap, explorer scoring, and report behavior before source changes. r[chaoscontrol.coverage_symbols.mapping] r[chaoscontrol.coverage_symbols.report]
- [x] [serial] Inventory the build and packaging surfaces that can emit a symbol catalog and preserve it with the guest closure. r[chaoscontrol.coverage_symbols.catalog]
- [x] [serial] Publish the versioned symbol-catalog format, source-root rules, completeness states, and link-time-optimization handling. r[chaoscontrol.coverage_symbols.catalog] r[chaoscontrol.coverage_symbols.nonclaims]

## Phase 2: Build and runtime facts

- [ ] [serial] Emit bounded symbol catalogs for the admitted Rust workload modules and preserve their exact identities with the guest artifacts. r[chaoscontrol.coverage_symbols.catalog]
- [x] [serial] Add a bounded SDK declaration API for loaded module content identity, reviewed path, and load ordinal. r[chaoscontrol.coverage_symbols.module_identity]
- [ ] [serial] Wire automatic module-load declarations and registered instrumentation identifiers into the admitted guest runtime. r[chaoscontrol.coverage_symbols.module_identity]
- [x] [parallel] Add negative build fixtures for LTO/incomplete catalogs and path escape. r[chaoscontrol.coverage_symbols.catalog] r[chaoscontrol.coverage_symbols.fixtures]

## Phase 3: Pure mapping and report

- [x] [serial] Implement pure catalog admission, module matching, identifier/address mapping, and mapped/unmapped accounting. r[chaoscontrol.coverage_symbols.mapping]
- [x] [serial] Implement the bounded symbolized report and descriptive failing-versus-control ranking. r[chaoscontrol.coverage_symbols.report]
- [x] [parallel] Add positive mapping fixtures for Rust edges, KCOV program counters, and shared-module loads. r[chaoscontrol.coverage_symbols.mapping] r[chaoscontrol.coverage_symbols.fixtures]
- [x] [parallel] Add negative mapping fixtures for unknown modules, duplicate identities, accounting mismatch, and over-bound input. r[chaoscontrol.coverage_symbols.mapping] r[chaoscontrol.coverage_symbols.fixtures]

## Phase 4: Evidence and real cohort

- [x] [serial] Emit the bounded receipt binding catalogs, module loads, observation sources, limits, accounting, and report digest. r[chaoscontrol.coverage_symbols.evidence]
- [ ] [serial] Exercise one admitted KVM workload and report exact symbolized coverage plus unaccounted observations. r[chaoscontrol.coverage_symbols.module_identity] r[chaoscontrol.coverage_symbols.report]
- [ ] [parallel] Add negative evidence fixtures for catalog drift, module drift, changed observation source, and causal overclaim. r[chaoscontrol.coverage_symbols.evidence] r[chaoscontrol.coverage_symbols.nonclaims]

## Phase 5: Verification and evidence

- [ ] [serial] Run focused build, SDK, VMM, explorer, and evidence tests; Clippy with warnings denied; octet; Cairn validation and gates; and relevant Nix checks. r[chaoscontrol.coverage_symbols.fixtures]
- [ ] [serial] Record the bounded KVM receipt, source and artifact identities, positive and negative results, and remaining non-claims. r[chaoscontrol.coverage_symbols.evidence] r[chaoscontrol.coverage_symbols.nonclaims]
