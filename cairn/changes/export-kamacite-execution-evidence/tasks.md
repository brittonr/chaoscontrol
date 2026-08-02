## Phase 1: Foundation and profile binding

- [ ] [serial] I1 Record baseline profile, evidence, replay, projection, workspace, and lifecycle results. r[chaoscontrol.kamacite_execution_export.fixtures]
- [ ] [depends:add-nickel-simulator-campaign-profiles] I2 Extend Nickel simulator and campaign profiles with optional exact Kamacite identity bindings and export policy. r[chaoscontrol.kamacite_execution_export.profile_binding]
- [ ] [depends:published-kamacite-deterministic-execution-profile] I3 Add frozen Kamacite schemas, projections, identity domains, and conformance fixtures. r[chaoscontrol.kamacite_execution_export.profile_binding] r[chaoscontrol.kamacite_execution_export.boundary]

## Phase 2: Pure core and runtime shell

- [ ] [depends:kamacite-execution-profile-binding] I4 Implement pure mapping admission, choice and fault record validation, replay-parent checks, projection planning, and deterministic diagnostics. r[chaoscontrol.kamacite_execution_export.runtime_records] r[chaoscontrol.kamacite_execution_export.fault_states] r[chaoscontrol.kamacite_execution_export.effect_mapping] r[chaoscontrol.kamacite_execution_export.replay_linkage]
- [ ] [depends:kamacite-execution-export-core] I5 Extend runtime shells to collect bounded records and write the compatibility projection only after successful validation. r[chaoscontrol.kamacite_execution_export.projection]
- [ ] [depends:kamacite-execution-export-core] I6 Link optional product property receipts without creating, interpreting, or promoting them. r[chaoscontrol.kamacite_execution_export.property_pair]

## Phase 3: Verification and closeout

- [ ] [parallel] V1 Add positive fixtures for profile binding, complete choices, separate fault states, effect lowering, snapshot replay, and property links. r[chaoscontrol.kamacite_execution_export.fixtures]
- [ ] [parallel] V2 Add negative fixtures for stale profiles, identity cycles, unknown operations, inferred mappings, collapsed fault states, missing parents, tampered snapshots, role promotion, and missing KVM. r[chaoscontrol.kamacite_execution_export.fixtures] r[chaoscontrol.kamacite_execution_export.boundary]
- [ ] [parallel] V3 Add property tests for ordering, fault-state transitions, identity sensitivity, complete parent binding, and deterministic diagnostics. r[chaoscontrol.kamacite_execution_export.runtime_records] r[chaoscontrol.kamacite_execution_export.fault_states] r[chaoscontrol.kamacite_execution_export.replay_linkage]
- [ ] [serial] I7 Add one KVM-free export rail and one bounded KVM producer rail with explicit blocked results. r[chaoscontrol.kamacite_execution_export.rails]
- [ ] [serial] I8 Document profile use, workload mappings, effect strata, paired receipts, rails, migration, and non-claims. r[chaoscontrol.kamacite_execution_export.boundary]
- [ ] [depends:kamacite-execution-export-fixtures] V4 Run focused tests, profile freshness, evidence checks, Clippy, Cairn validation, gates, KVM-free Nix checks, and one bounded KVM smoke when available. r[chaoscontrol.kamacite_execution_export.fixtures] r[chaoscontrol.kamacite_execution_export.rails]

## Verification Coverage

- `Scenario: Complete profile binding passes` -> I2, I3, I4, V1
- `Scenario: Stale Kamacite profile fails before launch` -> I4, V2
- `Scenario: Runtime back-reference fails` -> I4, V2
- `Scenario: Fault states remain distinct` -> I4, V1, V2, V3
- `Scenario: Inferred product operation fails` -> I4, V2
- `Scenario: Snapshot-backed replay export passes` -> I4, I5, V1
- `Scenario: Product property link stays external` -> I6, V1, V2
- `Scenario: Static export rail requires no KVM` -> I7, V1
- `Scenario: Missing KVM is blocked` -> I7, V2
- `Scenario: Universal determinism claim fails` -> I5, V2, I8
