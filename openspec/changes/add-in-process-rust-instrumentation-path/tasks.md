## Phase 1: Specification Foundation

- [x] [serial] Create the advanced in-process Rust instrumentation proposal, design, task plan, and delta spec.
- [ ] [parallel] Review existing Rust workload template docs and local-output summary fields for where adoption track labels should appear.

## Phase 2: Local In-Process Sample

- [ ] [serial] Add a minimal downstream-shaped Rust sample with a service module, external driver, and feature/config-gated service-internal SDK assertions.
- [ ] [depends:local-sample] Add positive tests proving enabled in-process instrumentation emits local SDK output with the expected adoption track label.
- [ ] [depends:local-sample] Add negative/default tests proving the sample builds or runs with service-internal instrumentation disabled.

## Phase 3: Reporting and Readiness Boundaries

- [ ] [depends:local-sample] Extend local-output summary/report generation to distinguish `external-harness` and `in-process-service` observations without changing accepted replay proof classification.
- [ ] [depends:reporting] Update readiness/docs wording so in-process local evidence is framed as deeper instrumentation coverage, not snapshot-backed replay proof.
- [ ] [depends:reporting] Add report fixture coverage for harness-only, in-process-only, and mixed-track local outputs.

## Phase 4: Documentation and Verification

- [ ] [depends:reporting] Document the two-track adoption path, including escalation criteria and explicit non-goals.
- [ ] [depends:docs] Run focused local harness smoke, sample tests, summary/report fixture checks, `openspec validate add-in-process-rust-instrumentation-path --strict`, and `git diff --check`.
- [ ] [depends:verification] Decide whether to add an optional VM proof example after local track labeling is proven; do not require a kernel/KVM run for the initial docs/local slice.
