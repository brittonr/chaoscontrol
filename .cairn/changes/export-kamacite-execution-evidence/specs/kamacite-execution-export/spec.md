# Kamacite Execution Export Specification Delta

## ADDED Requirements

### Requirement: Campaigns bind exact Kamacite execution profiles

r[chaoscontrol.kamacite_execution_export.profile_binding]

An opted-in ChaosControl campaign or replay export MUST bind one canonical Kamacite deterministic execution profile and compatibility projection identity.

The binding MUST name the application-host binding, closed effect summary, handler cohort, scheduler policy, fault policy, schema, bounds, and non-claims.

Only pre-run artifacts MAY enter profile identity. The dependency graph MUST remain acyclic.

#### Scenario: Complete profile binding passes

- GIVEN a simulator or campaign profile carries current exact Kamacite identities and a supported export policy
- WHEN profile and Rust admission run
- THEN the binding MUST pass before any portable runtime record is emitted.

#### Scenario: Stale Kamacite profile fails before launch

- GIVEN a Kamacite profile, operation, handler, summary, projection, or schema identity is stale or mismatched
- WHEN ChaosControl admits the opted-in campaign
- THEN admission MUST fail before VM or simulator construction with a deterministic stale-kamacite-profile diagnostic.

#### Scenario: Runtime back-reference fails

- GIVEN a profile identity includes a run record, trace, or projection whose identity already depends on that profile
- WHEN ChaosControl validates profile dependencies
- THEN validation MUST fail with a deterministic kamacite-profile-identity-cycle diagnostic.

### Requirement: Runtime records expose portable execution fields

r[chaoscontrol.kamacite_execution_export.runtime_records]

Rust-owned runtime records MUST expose bounded fields for Kamacite choice, fault, and effect-log projections.

Records MUST bind exact profile, operation, handler, step, actor, generation, logical time, causal parent, input, output, and applicable fault identities.

#### Scenario: Complete choice record passes

- GIVEN a scheduler decision records the complete eligible set, selected alternative, schedule position, actor, logical time, and causal parents
- WHEN the pure export core validates the record
- THEN the choice projection plan MUST pass with deterministic canonical ordering.

#### Scenario: Choice record omits alternatives

- GIVEN a scheduler decision records only a seed or selected value and omits the eligible alternatives required by policy
- WHEN the pure export core validates the record
- THEN validation MUST fail with a deterministic incomplete-choice-record diagnostic.

### Requirement: Fault states remain separate runtime facts

r[chaoscontrol.kamacite_execution_export.fault_states]

ChaosControl MUST record scheduled, attempted, applied, and observed fault states separately for every exported fault application.

A prior state MUST NOT satisfy a later state. Each non-applied or non-observed result MUST retain a typed reason.

#### Scenario: Fault states remain distinct

- GIVEN a scheduled fault is attempted but the target rejects it
- WHEN ChaosControl emits the runtime record
- THEN scheduled and attempted MUST be true, applied and observed MUST be false, and the rejection reason MUST be present.

#### Scenario: Collapsed success state fails

- GIVEN a runtime record uses one success value for attempted, applied, and observed states
- WHEN export validation runs
- THEN validation MUST fail with a deterministic collapsed-fault-state diagnostic.

### Requirement: Product and host effects require exact adapters

r[chaoscontrol.kamacite_execution_export.effect_mapping]

ChaosControl MUST require explicit workload adapter mappings between product semantic operations and runtime or host operations.

It MUST NOT infer product operations from packet, block, process, clock, interrupt, syscall, name, schema, or timing similarity.

#### Scenario: Explicit effect lowering passes

- GIVEN a workload adapter binds an exact product operation to declared runtime or host operations for one bounded context
- WHEN mapping admission runs
- THEN the link MAY enter the export with directional and non-equivalence claims.

#### Scenario: Inferred product operation fails

- GIVEN a runtime record observes a packet or block event without an exact workload adapter mapping
- WHEN export attempts to label it as a product semantic operation
- THEN validation MUST fail with a deterministic inferred-semantic-operation diagnostic.

### Requirement: Replay exports bind complete parent evidence

r[chaoscontrol.kamacite_execution_export.replay_linkage]

A replay export MUST bind the execution profile, choice trace, effect logs, fault receipts, workload artifact, VM cohort, snapshot, and replay verdict.

Existing protocol-required snapshot SHA-256 identities MUST remain unchanged. New ChaosControl-owned profile and projection identities MUST use BLAKE3.

#### Scenario: Snapshot-backed replay export passes

- GIVEN a replay verdict is `snapshot_backed_reproduced` and every required profile, trace, fault, artifact, VM, snapshot, and verdict identity matches
- WHEN export validation runs
- THEN replay linkage MUST pass without promoting the verdict beyond its existing bounded claim.

#### Scenario: Replay parent is missing

- GIVEN a replay verdict omits or mismatches a required choice, fault, effect-log, artifact, VM, or snapshot parent
- WHEN export validation runs
- THEN validation MUST fail with a deterministic incomplete-replay-linkage diagnostic.

### Requirement: Product property receipts remain external

r[chaoscontrol.kamacite_execution_export.property_pair]

ChaosControl MAY reference a product-owned property receipt through exact subject, profile, trace, and observation identities.

ChaosControl MUST NOT create its semantic verdict, interpret its invariant, or merge its verification role with run or replay evidence.

#### Scenario: Product property link stays external

- GIVEN a product property receipt binds the same subject and trace as a ChaosControl replay receipt
- WHEN ChaosControl emits the link
- THEN both receipt identities, producers, roles, supported claims, and non-claims MUST remain separate.

#### Scenario: Runtime receipt claims product property

- GIVEN an export attempts to use a ChaosControl run or replay receipt as the product property receipt
- WHEN validation runs
- THEN validation MUST fail with a deterministic property-role-substitution diagnostic.

### Requirement: Portable projection follows ChaosControl validation

r[chaoscontrol.kamacite_execution_export.projection]

ChaosControl MUST emit a deterministic compatibility projection only after its pure core validates every selected runtime record and parent identity.

The projection MUST identify Kamacite as canonical Preserves owner and Valence as Evidence IR linkage owner.

#### Scenario: Projection is stable

- GIVEN identical admitted profile, mapping, runtime record, artifact, and verdict inputs
- WHEN ChaosControl emits the compatibility projection twice
- THEN canonical rows and the BLAKE3 projection identity MUST match.

#### Scenario: Invalid record blocks projection

- GIVEN any selected record is malformed, stale, incomplete, over-limit, or overclaiming
- WHEN the shell requests projection output
- THEN the shell MUST refuse successful output and report deterministic diagnostics.

### Requirement: Static and KVM rails remain separate

r[chaoscontrol.kamacite_execution_export.rails]

ChaosControl MUST provide a default KVM-free rail for profile, mapping, projection, fixture, and non-claim validation.

A separate bounded KVM rail MUST produce fresh runtime records. Missing KVM or unsupported host facts MUST produce `blocked`, not pass.

#### Scenario: Static export rail requires no KVM

- GIVEN checked-in profile and runtime-record fixtures
- WHEN the default export rail runs on a host without KVM
- THEN static validation MUST run without claiming fresh VM behavior.

#### Scenario: Missing KVM is blocked

- GIVEN the bounded producer rail requires KVM and the host lacks usable KVM access
- WHEN the producer rail runs
- THEN the result MUST be `blocked` with a typed reason and MUST NOT count as passing runtime evidence.

### Requirement: Export claims remain bounded

r[chaoscontrol.kamacite_execution_export.boundary]

A passing export MUST prove only that validated ChaosControl records match one admitted portable projection shape and exact parent identities.

It MUST NOT prove universal determinism, semantic equivalence, replay completeness, fault effect, property truth, VMM correctness, or release eligibility.

#### Scenario: Universal determinism claim fails

- GIVEN a projection, receipt, report, diagnostic, or document claims universal determinism or product correctness
- WHEN boundary validation runs
- THEN validation MUST fail with a deterministic kamacite-export-overclaim diagnostic.

### Requirement: Export fixtures cover success and failure

r[chaoscontrol.kamacite_execution_export.fixtures]

ChaosControl MUST include positive, negative, property, compatibility, and frozen Kamacite fixtures for the export path.

#### Scenario: Frozen Kamacite cohort drifts

- GIVEN a Kamacite profile, schema, operation, handler, role, identity, or non-claim field changes
- WHEN ChaosControl validates the frozen cohort
- THEN validation MUST fail until an explicit reviewed cohort update binds the new artifacts.
