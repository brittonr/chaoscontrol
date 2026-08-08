# Semantic History Specification

## Purpose

Defines the `semantic-history` capability.

## Requirements

### Requirement: Semantic histories preserve event order
r[chaoscontrol.semantic_history.schema] ChaosControl MUST define a versioned semantic history with separate invocation and completion events. Each event MUST bind stable order, operation, attempt, process, function, object, value, controller-time, profile, and source identities. Completion events MUST pair with one prior invocation.

#### Scenario: Concurrent operations retain overlap
- GIVEN two operations overlap in controller time
- WHEN history admission pairs their invocation and completion events
- THEN it MUST preserve both intervals without ordering them by completion alone.

#### Scenario: Completion has no invocation
- GIVEN a completion event names no admitted prior invocation
- WHEN history admission runs
- THEN it MUST reject the history with a stable pairing diagnostic.

### Requirement: Outcome semantics preserve uncertainty
r[chaoscontrol.semantic_history.outcomes] ChaosControl MUST define `ok`, `fail`, `info`, and pending operation outcomes. `ok` MUST require an effect, `fail` MUST exclude an effect, and `info` MAY include an effect. Pending invocations MUST remain incomplete unless explicit finalization records the conversion policy and reason. Retries MUST preserve logical identity and use distinct attempt identities.

#### Scenario: Lost acknowledgement remains uncertain
- GIVEN a write can commit before its client disconnects
- WHEN the client records `info`
- THEN the checker MUST consider histories where the write took effect and histories where it did not.

#### Scenario: Retry changes logical identity
- GIVEN one retry represents the same logical operation with a new logical ID
- WHEN retry validation runs
- THEN it MUST reject the trace as an invalid idempotency input.

### Requirement: History v2 has canonical BLAKE3 identity
r[chaoscontrol.semantic_history.identity] ChaosControl MUST compute history v2 identity from a versioned canonical semantic projection with domain-separated BLAKE3. Identity MUST bind event order, event content, model, profile, bounds, and completeness accounting. JSON field order MUST NOT affect identity.

#### Scenario: Transport fields are reordered
- GIVEN two JSON values represent the same admitted semantic history with different object-field order
- WHEN v2 identity is computed
- THEN both values MUST produce the same semantic history ref.

#### Scenario: One outcome changes
- GIVEN one admitted history changes an operation from `info` to `fail`
- WHEN v2 identity is computed
- THEN its semantic history ref MUST change.

### Requirement: Linearizability uses real-time order
r[chaoscontrol.semantic_history.linearizability] The checker MUST derive real-time precedence from invocation and completion intervals. It MUST search legal model transitions that preserve this precedence. It MUST return `valid`, `invalid`, or `unknown`. Search, evidence, model, or resource bounds MUST NOT become `valid` when evaluation is incomplete.

#### Scenario: Overlapping writes can linearize in either order
- GIVEN two writes overlap and a later read returns a value allowed by one legal order
- WHEN linearizability evaluation runs
- THEN it MUST accept a legal order without requiring completion order.

#### Scenario: Search bound is exhausted
- GIVEN a well-formed history exceeds an admitted search bound
- WHEN linearizability evaluation stops
- THEN it MUST return `unknown` with the exact exhausted bound.

### Requirement: First-party models have pure transitions
r[chaoscontrol.semantic_history.models] ChaosControl MUST provide pure read/write register and compare-and-swap models. A model MUST define canonical initial state, admitted operations, transition results, and state identity. Independent-key decomposition MUST require an explicit key-isolation declaration and complete key admission.

#### Scenario: Compare-and-swap observes expected state
- GIVEN the register contains the expected value
- WHEN an admitted compare-and-swap operation succeeds
- THEN the model MUST move to the replacement value and return the declared success result.

#### Scenario: Cross-key model lacks isolation
- GIVEN a model does not declare key isolation
- WHEN a profile requests independent-key decomposition
- THEN admission MUST reject decomposition rather than weaken the model.

### Requirement: Reports retain bounded witnesses
r[chaoscontrol.semantic_history.witness] A valid report MUST retain one legal linearization witness. An invalid report MUST retain the failing operation set, relevant model states, and violated transition. A reducer MUST preserve history well-formedness and failure class. It MUST classify its output as minimal, locally reduced, or budget-limited.

#### Scenario: Invalid history is reduced
- GIVEN an invalid history contains unrelated operations
- WHEN bounded reduction removes operations
- THEN every retained candidate MUST remain well formed and preserve the original invalid failure class.

#### Scenario: Reduction budget expires
- GIVEN reduction stops before it proves minimality
- WHEN the report is emitted
- THEN it MUST label the witness as budget-limited and MUST NOT call it minimal.

### Requirement: Reference conformance remains independent
r[chaoscontrol.semantic_history.reference_conformance] ChaosControl MUST support a pinned external reference-checker conformance rail through an explicit history adapter. The external tool MUST remain outside the pure checker core. Native and reference disagreement MUST block promotion and retain both reports.

#### Scenario: Native and reference verdicts agree
- GIVEN an admitted conformance corpus and compatible model semantics
- WHEN both checkers evaluate each history
- THEN the conformance report MUST bind both tool identities and matching verdicts.

#### Scenario: Reference checker disagrees
- GIVEN the native and reference checkers return different verdicts
- WHEN conformance classification runs
- THEN it MUST report a blocker and MUST NOT choose either verdict as authoritative.

### Requirement: History v1 remains bounded legacy evidence
r[chaoscontrol.semantic_history.compatibility] ChaosControl MAY read history and report v1 for compatibility. It MUST preserve existing v1 digest semantics and limitations. A v1 completion-order result MUST NOT satisfy a v2 linearizability requirement.

#### Scenario: Legacy fixture is read
- GIVEN a valid history v1 fixture
- WHEN the compatibility reader loads it
- THEN it MUST preserve the legacy model, digest, and non-linearizability limitation.

#### Scenario: Legacy pass is promoted
- GIVEN a v1 completion-order report has a pass verdict
- WHEN v2 evidence admission runs
- THEN it MUST reject the report as insufficient for linearizability.

### Requirement: Semantic timelines use admitted facts
r[chaoscontrol.semantic_history.timeline] ChaosControl MUST derive operation, lifecycle, fault, latency, and witness timelines from one pure projection. A fault-effect band MUST require admitted applied or observed records. Temporal overlap MUST NOT be presented as causation.

#### Scenario: Fault was selected but not observed
- GIVEN a fault schedule selected an attempt without an admitted effect observation
- WHEN the timeline is rendered
- THEN it MUST show selection separately and MUST NOT show an observed fault band.

#### Scenario: Text and HTML views render one report
- GIVEN one admitted semantic projection
- WHEN text and static HTML renderers run
- THEN both views MUST retain the same event identities, phases, verdict, and witness membership.

### Requirement: Semantic evidence remains claim-scoped
r[chaoscontrol.semantic_history.evidence] A semantic report MUST bind history, model, profile, checker, bounds, completeness, verdict, witness, reduction, reference status, and non-claims. A valid verdict MUST NOT imply system correctness, checker soundness, exhaustive schedules, deterministic replay, fault effect, durability, transactions, security, or release readiness.

#### Scenario: Complete finite report is admitted
- GIVEN a report has complete identities, finite bounds, admitted history, and a terminal checker verdict
- WHEN evidence validation runs
- THEN it MUST accept only the exact finite model-evaluation claim.

#### Scenario: Valid verdict becomes a universal claim
- GIVEN a consumer labels one valid finite history as universal system correctness
- WHEN claim-boundary validation runs
- THEN it MUST reject the promoted claim.

### Requirement: Semantic logic has a functional core
r[chaoscontrol.semantic_history.boundary] Event admission, pairing, canonicalization, model transitions, predecessor construction, search, verdict classification, reduction, and timeline projection MUST be pure deterministic logic. Transport, VMM, filesystem, process, external-tool, persistence, and rendering effects MUST remain in shells.

#### Scenario: Checker runs without infrastructure
- GIVEN an admitted in-memory history, model, and bounds
- WHEN semantic evaluation runs
- THEN it MUST return the same result without files, environment, processes, network, KVM, wall clocks, or output effects.

### Requirement: Semantic history validation covers success and failure
r[chaoscontrol.semantic_history.validation] The change MUST include positive and negative validation for schemas, pairing, outcomes, identities, models, concurrency, bounds, witnesses, compatibility, reference disagreement, timelines, and claim boundaries.

#### Scenario: Validation corpus runs
- GIVEN valid, invalid, unknown, malformed, legacy, and overclaim fixtures
- WHEN the selected validation rail runs
- THEN valid fixtures MUST pass and each negative fixture MUST fail with its expected stable class.
