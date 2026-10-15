# Replay Debugger Artifacts Specification

## Purpose

Reload and navigate a retained ChaosControl execution in a fresh process using explicit validated snapshots, complete deterministic inputs, and the same exact continuation checks as recording replay.

## ADDED Requirements

### Requirement: Exact sessions persist complete bounded replay inputs

r[chaoscontrol.replay_debugger.session] An exact debugger session MUST persist a versioned envelope containing its finite tick interval, complete deterministic execution configuration and topology, seed and fault-run identity, guest artifact content identities, ordered checkpoint inventory with snapshot references, and canonical fault/SMP continuation evidence. A finding-derived session MUST additionally bind the selected v2 assertion/catalog and bug provenance. Omitted deterministic inputs MUST NOT be reconstructed from defaults or logs. New session identities MUST use domain-separated canonical BLAKE3; existing snapshot-reference digest and codec meanings MUST remain unchanged.

#### Scenario: Non-default SMP session is reloaded

- GIVEN a completed session with a non-default vCPU topology, instruction quantum, clock/progress policy, and retained snapshot references
- WHEN a new process admits the session
- THEN it MUST recover the exact declared inputs and checkpoint inventory without relying on creator-process memory or default VM settings.

#### Scenario: Deterministic configuration is incomplete

- GIVEN a session omits a required progress, device, topology, artifact identity, or run binding
- WHEN exact-session admission runs
- THEN it MUST reject the missing input and MUST NOT silently supply a default controller configuration.

### Requirement: One shared implementation owns persisted snapshots

r[chaoscontrol.replay_debugger.shared_store] Replay intake and explorer reproduction MUST use one implementation of the current snapshot store, envelope codec, digest verification, and size/schema checks owned by `chaoscontrol-replay`. The cutover MUST migrate all explorer callers and remove the old module without a compatibility re-export or duplicate reader. Neutral evidence reference DTOs MUST remain independent of filesystem and KVM effects.

#### Scenario: Existing retained artifact is used by both consumers

- GIVEN a valid current-codec snapshot artifact and its exact existing reference
- WHEN explorer reproduction and replay session intake load it through their shared owner
- THEN both MUST restore the same snapshot state under the same validation rules and MUST preserve the artifact's encoded identity.

#### Scenario: Current artifact is corrupt

- GIVEN an artifact whose bytes no longer match its existing reference
- WHEN either consumer requests it
- THEN intake MUST reject it before VM mutation and MUST NOT accept it through a second or legacy reader.

### Requirement: Session admission precedes machine effects

r[chaoscontrol.replay_debugger.admission] The session validator MUST purely determine restore and continuation eligibility from supplied metadata, decoded state facts, capability facts, artifact digest observations, target, and named finite limits. The shell MUST enforce bounded no-follow capability-rooted reads and validate every selected artifact, topology, schema, run/ledger binding, and target before creating or mutating a VM. Missing or incompatible state MUST NOT trigger remote fetching or boot fallback for a requested checkpoint.

#### Scenario: Artifact bytes move under an admitted root

- GIVEN the exact retained bytes are explicitly relocated under a new admitted artifact root without changing their identities
- WHEN the session validates the remapped locators
- THEN the same restore plan MUST be admitted without encoding the former host path as content identity.

#### Scenario: Locator escapes or target exceeds coverage

- GIVEN a selected locator is absolute, traversing, or symlinked outside the root, or the target is outside the recorded interval
- WHEN session intake runs
- THEN it MUST return a specific blocker before KVM creation or snapshot mutation and MUST NOT reinterpret the request as boot replay.

#### Scenario: Input exceeds a named limit

- GIVEN the manifest, checkpoint table, trace, decoded snapshot, artifact count, or requested tick interval exceeds its admitted bound or overflows arithmetic
- WHEN admission runs
- THEN it MUST fail before allocating or executing beyond that bound.

### Requirement: Debugger navigation has exact replay validation parity

r[chaoscontrol.replay_debugger.navigation] Read-only debugger navigation and exact recording replay MUST share the same continuation path, validating actual per-round fault outcomes and SMP schedule traces, checkpoint prefix bindings, and arrival at the exact requested tick. Checkpoint choice MUST use deterministic tick/ID ordering. A displayed verified position MUST be committed only after all selected checks pass.

#### Scenario: Backward and forward navigation crosses a checkpoint

- GIVEN a fresh process loaded a valid session with multiple persisted checkpoints
- WHEN it rewinds and advances through the same declared target ticks as exact replay
- THEN restored memory/register observations and canonical continuation evidence MUST agree with the corresponding exact replay control.

#### Scenario: Schedule evidence diverges without a fault mismatch

- GIVEN actual SMP schedule evidence differs from the recording while fault observations still agree
- WHEN either exact replay or debugger navigation reaches the affected round
- THEN both MUST reject that continuation and MUST NOT publish the requested position as verified.

### Requirement: Existing reproduction can emit a retained debugger session

r[chaoscontrol.replay_debugger.producer] The existing explorer reproduction path MUST accept `--debug-recording-output` to retain a bounded fresh session from a validated replay parent. It MUST preserve original bug/parent provenance, capture the session-start checkpoint after explicit counterfactual-run setup and before guest progress, record the new run's exact completed-round evidence, and publish a completed manifest only after reference closure validates. Existing reproduction verdict and exit semantics MUST remain unchanged.

#### Scenario: Producer exits before inspection begins

- GIVEN an admitted Rust finding reproduced with debugger recording enabled
- WHEN the producer exits and separate replay and inspect processes open its completed session
- THEN both MUST restore a retained checkpoint and exercise the recorded continuation without access to the producer's in-memory state.

#### Scenario: Output publication fails

- GIVEN capture or publication of a required snapshot, trace, or manifest fails
- WHEN reproduction handles that failure
- THEN it MUST identify the recording failure, MUST NOT expose a completed session referencing missing state, and MUST preserve any previously completed output.

### Requirement: Failed navigation cannot expose unverified live state

r[chaoscontrol.replay_debugger.failure_isolation] Divergence, premature stop, post-entry evidence failure, controller poison, or an operational watchdog timeout MUST invalidate the attempted live position. The debugger MUST NOT serve that state as a verified observation or reuse the poisoned runner. A subsequent exact operation MUST begin from a freshly admitted restore on a fresh runner. Last verified metadata MUST remain distinguishable from failed live state.

#### Scenario: Execution stops before the requested tick

- GIVEN an admitted continuation ends before its requested target
- WHEN navigation classifies the result
- THEN it MUST return an incomplete-continuation failure rather than showing the target tick or inferring a successful guest verdict.

#### Scenario: Operator tries again after poison

- GIVEN the previous attempt poisoned a controller after guest progress
- WHEN another seek is requested
- THEN the old runner MUST NOT execute, restore, or expose state, and only a fresh independently admitted runner MAY attempt the new seek.

### Requirement: Inspection and counterfactual evidence stay distinct

r[chaoscontrol.replay_debugger.claim_boundary] Metadata-only historical input MUST remain diagnostic when exact-session prerequisites are absent. Explicit schedule, memory, or register mutations MUST be labeled counterfactual and MUST NOT certify the original recording's divergent suffix. A successful debugger seek or session export MUST NOT itself produce or promote `snapshot_backed_reproduced`, assertion-readiness coverage, workload correctness, cross-host equivalence, or universal determinism.

#### Scenario: Historical JSON has no persisted checkpoint inventory

- GIVEN an old recording has inspectable metadata but lacks exact-session snapshots or execution inputs
- WHEN metadata commands and exact-session commands consume it
- THEN metadata inspection MAY remain available, but exact restore MUST fail with the missing prerequisites rather than manufacture a snapshot.

#### Scenario: Memory is deliberately modified

- GIVEN an operator edits memory in a restored session
- WHEN the UI or CLI describes the resulting state
- THEN it MUST mark it counterfactual and MUST NOT describe a divergent continuation as verified original-run replay.

### Requirement: Validation covers fresh-process behavior and adversarial intake

r[chaoscontrol.replay_debugger.validation] Implementation acceptance MUST include paired positive/negative portable fixtures and an actual bounded KVM reproduction-export-process-exit-reload-navigation workflow on an admitted Rust workload. It MUST exercise non-default SMP configuration, memory/register observations, exact start/end bounds, corruption and missing artifacts, identity/trace tampering, publication failure, and operational/poison handling. Evidence MUST bind exact commands, source/config/artifact identities, bounds, and host profile; affected deep lineages and unavailable KVM MUST remain explicit blockers rather than passes.

#### Scenario: Bounded KVM workflow succeeds

- GIVEN fresh admitted artifacts and a host meeting the selected exact progress profile
- WHEN reproduction records a session and separate CLI processes replay, rewind, advance, and inspect it
- THEN their canonical continuation and declared inspected values MUST match the retained control within the recorded horizon.

#### Scenario: Required evidence cannot run

- GIVEN a selected deep snapshot is invalid or the KVM/capability/artifact prerequisite is unavailable
- WHEN acceptance is evaluated
- THEN the affected workflow MUST remain blocked with its exact missing prerequisite, and portable tests or Cairn gates MUST NOT be substituted for execution evidence.
