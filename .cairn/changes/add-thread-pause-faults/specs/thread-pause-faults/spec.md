# Thread Pause Fault Specification

## Purpose

Define a deterministic, cooperative thread-level pause fault for an admitted guest runtime cohort, including its identity, safe-point boundary, release, snapshot, evidence, and claim limits.

## ADDED Requirements

### Requirement: Logical thread identity is stable

r[chaoscontrol.thread_pause.identity] The guest runtime MUST assign each cooperative thread a stable logical identity derived from its process identity, a monotonic thread generation, and an ordinal. Host thread IDs, virtual addresses, and recycled kernel identifiers MUST NOT be used as durable identity. Snapshots and exact replay MUST preserve the identity-to-runtime mapping and reject a restored or observed thread whose identity is unknown, duplicated, or generation-mismatched.

#### Scenario: A thread is paused and restored

- GIVEN a running cooperative thread with an assigned logical identity
- WHEN a snapshot is taken and restored
- THEN the same logical identity MUST resolve to the same logical thread
- AND a reused or mismatched generation MUST be rejected.

#### Scenario: A host thread ID is supplied

- GIVEN a fault target names only an OS thread ID or address
- WHEN thread-target admission runs
- THEN it MUST reject the target as non-stable.

### Requirement: Pauses take effect only at cooperative safe points

r[chaoscontrol.thread_pause.safepoints] An instrumented thread MUST check the pause gate only at declared safe points in the admitted runtime. A pause MUST NOT use an asynchronous host signal, `ptrace`, wall-clock sleep, or host scheduler intervention. The runtime MUST publish the safe-point and pause-gate ABI through a versioned identity, and a guest build without that identity MUST be rejected for this fault.

#### Scenario: A pause targets an instrumented thread

- GIVEN an admitted runtime and a thread that reaches a declared safe point
- WHEN the pause window is active
- THEN the thread MUST become not runnable at that safe point
- AND another runnable thread MAY make progress under the deterministic scheduler.

#### Scenario: A pause targets an uninstrumented thread

- GIVEN a workload that does not expose the admitted safe-point ABI
- WHEN a thread-pause fault is admitted
- THEN the planner MUST return a typed unsupported-capability result
- AND no signal or sleep fallback MUST run.

### Requirement: Thread-pause faults are typed and bounded

r[chaoscontrol.thread_pause.fault] The fault catalogue MUST define one thread-pause variant that names an exact logical thread, a finite virtual-time window, and the owning process or role. Admission MUST enforce named limits for active paused threads, pause events per run, pause duration, total paused-thread ticks, and target uniqueness. Invalid, zero-length, overflowing, unknown, duplicate-active, or out-of-window targets MUST return typed rejection facts without applying an effect.

#### Scenario: A valid pause is admitted

- GIVEN a known thread identity and a finite window within the run horizon
- WHEN fault planning runs
- THEN the fault MUST receive an exact applicable plan
- AND the plan MUST NOT exceed any named pause bound.

#### Scenario: A duplicate active pause is requested

- GIVEN a thread with an active pause window
- WHEN another pause for the same thread is planned
- THEN the request MUST be rejected unless an explicitly admitted extension operation exists
- AND no second window MUST be applied.

### Requirement: Release and virtual-time advancement are deterministic

r[chaoscontrol.thread_pause.release] A paused thread MUST become runnable at the exact release tick. When at least one thread is runnable, the guest runtime scheduler MUST choose execution deterministically. When no thread is runnable, the runtime MUST request bounded deterministic virtual-time advancement rather than busy-wait on host time. Release ordering, scheduler choice, and advancement MUST be bound into the run evidence.

#### Scenario: Another thread remains runnable

- GIVEN one paused thread and one runnable thread
- WHEN the scheduler runs
- THEN only the runnable thread MUST execute until the pause release tick or a declared scheduling boundary.

#### Scenario: All threads are paused

- GIVEN every admitted cooperative thread is paused before its release tick
- WHEN the runtime cannot make guest progress
- THEN it MUST request the declared deterministic advancement path
- AND it MUST NOT spin on host time or skip the release tick.

### Requirement: Snapshot and replay preserve pause state

r[chaoscontrol.thread_pause.state] Snapshots MUST retain active pause windows, release ticks, logical thread identities and generations, scheduler runnability, and pending advancement requests. Restore MUST reject state that does not match an observed applied plan. Exact replay MUST reproduce the same pause observations and releases or reject the continuation.

#### Scenario: A snapshot is restored during a pause

- GIVEN a snapshot taken while a thread is paused
- WHEN the state is restored and continued
- THEN the same thread MUST remain paused until the recorded release tick
- AND the scheduler MUST reproduce the recorded runnable set.

#### Scenario: Pause evidence diverges

- GIVEN replay observes a different paused set, release tick, or thread generation
- WHEN exact validation runs
- THEN the continuation MUST be rejected
- AND it MUST NOT be reported as a reproduced replay.

### Requirement: Thread-pause evidence is bounded and staged

r[chaoscontrol.thread_pause.evidence] Each thread-pause attempt MUST record `Selected`, `Applicable`, `Applied`, `Observed`, `Released`, or the exact rejection/failure class. Evidence MUST bind the runtime ABI identity, target identity, plan window, actual first safe-point tick if observed, release tick, paused-thread tick count, and run identity, while excluding raw memory and logs beyond declared bounds.

#### Scenario: A pause is applied but never observed

- GIVEN a thread-pause plan whose target never reaches a safe point before the horizon
- WHEN the run ends
- THEN the ledger MUST show `Applied` without `Observed`
- AND it MUST NOT record a completed pause.

### Requirement: Thread-pause claims remain bounded

r[chaoscontrol.thread_pause.nonclaims] A thread-pause result MUST NOT claim arbitrary preemption, host thread control, race existence or absence, application correctness, complete interleaving coverage, replay promotion, or Antithesis parity. It MUST describe only the admitted cooperative runtime, logical target, window, and observed run facts.

#### Scenario: A passing race fixture is promoted beyond its scope

- GIVEN a race fixture that fails under one pause window
- WHEN a report claims general race coverage or correctness
- THEN non-claim validation MUST reject the report.

### Requirement: The thread-pause rail has positive and negative fixtures

r[chaoscontrol.thread_pause.fixtures] The rail MUST include positive fixtures for a deterministic two-thread interleaving, release at the exact tick, snapshot continuation, and replay parity, plus negative fixtures for an uninstrumented workload, unknown or reused identity, duplicate active pause, zero or overflowing window, safe-point starvation, snapshot drift, replay divergence, and overclaim.

#### Scenario: The rail is proposed for product use

- GIVEN the runtime ABI, fault variant, planner, scheduler, snapshot, evidence, and fixtures are complete
- WHEN focused Cargo, octet, Cairn, and Nix checks run
- THEN every positive fixture MUST pass
- AND every negative fixture MUST fail at its declared boundary.
