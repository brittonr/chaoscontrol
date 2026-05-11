## MODIFIED Requirements

### Requirement: Local-first replay-readiness product scope [r[replay-readiness-operator.local-rust-scope]]

The replay-readiness status surfaces MUST describe ChaosControl's current product target as Rust-only workload support on one machine with multiple local hypervisors, and MUST NOT present SaaS, cross-machine fleet scheduling, or multi-language SDK coverage as active missing features for current readiness.

#### Scenario: Status names current local scope [r[replay-readiness-operator.local-rust-scope.status]]

- GIVEN the generated replay-readiness status report is rendered
- WHEN an operator reviews experimental or unproven surfaces
- THEN the report identifies current missing product work in terms of local multi-hypervisor execution, local triage, Rust workload authoring, bounded determinism, and local artifact hygiene
- AND it labels hosted service, cross-machine fleet scheduling, and non-Rust SDKs as out-of-scope for current product readiness

#### Scenario: Hosted and fleet overclaims still fail [r[replay-readiness-operator.local-rust-scope.overclaim]]

- GIVEN a readiness report claims SaaS, real cross-machine fleet scheduling, or full Antithesis replacement from local evidence
- WHEN the promotion gate runs
- THEN it rejects the report even though those surfaces are current non-goals
