# Deterministic SMP Scheduling Specification

## Purpose

Select and resume ChaosControl vCPUs only at replay-stable execution boundaries while keeping host wall-clock watchdogs outside guest scheduling and verdict authority.

## ADDED Requirements

### Requirement: One pure state machine owns vCPU selection

r[chaoscontrol.deterministic_smp.schedule_core] Every active-vCPU change in deterministic SMP execution MUST be produced by a pure transition over explicit schedule state and replay-stable guest execution events.

#### Scenario: Identical schedule inputs select the same vCPU

r[chaoscontrol.deterministic_smp.validation.core]
- GIVEN identical active and runnable vCPUs, policy state, seeded-choice state, deterministic progress, quantum boundary, and guest event sequence
- WHEN the schedule core evaluates transitions
- THEN it MUST produce the same actions and state identities
- AND it MUST require no clock, signal, environment, KVM, thread, process, or output access.

#### Scenario: Invalid progress event is supplied

- GIVEN an event is stale, regresses progress, overshoots an exact boundary, or names an invalid vCPU
- WHEN the schedule core evaluates it
- THEN the transition MUST be rejected with a deterministic invariant diagnostic
- AND the prior schedule state MUST remain authoritative.

### Requirement: No-exit execution uses an exact deterministic boundary

r[chaoscontrol.deterministic_smp.progress_boundary] Deterministic SMP MUST provide a declared progress source that can regain control from a guest with no ordinary VM exits and stop at an exact replay-stable guest execution boundary.

#### Scenario: Guest vCPU spin-waits without VM exits

r[chaoscontrol.deterministic_smp.validation.spin_loop]
- GIVEN one runnable vCPU executes a no-exit spin loop while another runnable vCPU can make progress
- WHEN the active vCPU reaches the declared deterministic quantum boundary
- THEN the scheduler MUST switch according to deterministic policy at that boundary
- AND host elapsed time or watchdog arrival count MUST NOT select the boundary or destination vCPU.

#### Scenario: Accelerated progress source overshoots

- GIVEN a PMU-accelerated source reports progress beyond the exact boundary before correction
- WHEN the runtime validates the event
- THEN the run MUST terminate with a deterministic-progress error
- AND it MUST NOT reinterpret the observed host-time interruption as a valid boundary.

### Requirement: Host wall-clock events cannot mutate deterministic execution

r[chaoscontrol.deterministic_smp.wall_clock_isolation] Host signals, elapsed time, timer cadence, thread scheduling, and `EINTR` arrival order MUST NOT switch vCPUs, advance virtual time, increment deterministic progress, or change deterministic scheduler policy state.

#### Scenario: Spurious interrupts arrive

r[chaoscontrol.deterministic_smp.validation.spurious_interrupts]
- GIVEN arbitrary host `Intr` and `EINTR` events are interleaved with a fixed guest execution trace
- WHEN deterministic SMP handles them
- THEN the canonical vCPU transition trace and deterministic counters MUST match the trace without those host events.

### Requirement: Deterministic capability policy fails closed

r[chaoscontrol.deterministic_smp.capability_policy] ChaosControl MUST select a declared deterministic progress mode and validate its required capabilities before execution; it MUST NOT silently fall back to timer-only SMP preemption.

#### Scenario: Requested progress source is unavailable

- GIVEN the host lacks a capability required by the requested deterministic progress mode and no other deterministic mode was explicitly selected
- WHEN the VM is created
- THEN creation MUST fail with a typed capability diagnostic before guest execution
- AND no run artifact MAY classify the failed attempt as deterministic execution.

### Requirement: Watchdogs have abort authority only

r[chaoscontrol.deterministic_smp.watchdog] A host wall-clock watchdog MAY interrupt `KVM_RUN` to recover host control, but expiration MUST yield an operational timeout classification and MUST NOT fabricate a guest crash, panic, deadlock, schedule choice, or deterministic replay result.

#### Scenario: Host watchdog expires

- GIVEN guest execution does not return before the host watchdog deadline
- WHEN the watchdog interrupts execution
- THEN the shell MAY retry or stop with `HostWatchdogTimeout`
- AND deterministic guest state MUST not advance because of the timeout
- AND acceptance logic MUST treat the timeout as non-deterministic operational evidence.

### Requirement: Schedule evidence binds deterministic state transitions

r[chaoscontrol.deterministic_smp.schedule_evidence] Each deterministic progress or vCPU-switch record MUST identify the progress source, triggering guest boundary or event, selected action, and BLAKE3 identities of canonical pre-transition and post-transition schedule state.

#### Scenario: A vCPU switch is audited

- GIVEN a deterministic boundary causes scheduler policy to select another runnable vCPU
- WHEN the transition is recorded
- THEN the record MUST be sufficient to recompute and verify the transition from canonical state
- AND wall-clock timestamps and signal counts MUST NOT contribute to deterministic state identity.

### Requirement: Post-entry failures permanently poison the VM

r[chaoscontrol.deterministic_smp.vm_poison] After `KVM_RUN` can change guest state, any evidence, exit-handling, or schedule-action failure MUST permanently poison VM execution before returning.

#### Scenario: Exit handling fails after journal commit

r[chaoscontrol.deterministic_smp.validation.vm_poison]
- GIVEN an SMP instruction transition is committed to the bounded journal
- WHEN HLT handling, interrupt injection, or schedule-action application fails
- THEN the VM MUST retain the committed journal as diagnostic evidence
- AND every later execution, snapshot, restore, trace-drain, and success path MUST fail before mutation.

### Requirement: Complete schedule progress is snapshot-ready

r[chaoscontrol.deterministic_smp.snapshot_state] The scheduling owner MUST expose all active-vCPU, runnable-set, policy, seeded-choice, per-vCPU progress, quantum, and exact-step state required for whole-VM snapshot capture and restore.

#### Scenario: Snapshot resumes within a quantum

r[chaoscontrol.deterministic_smp.validation.snapshot]
- GIVEN a snapshot is captured after partial deterministic progress or during exact-step correction
- WHEN the complete VM snapshot owner restores that state
- THEN the next schedule transition MUST match uninterrupted execution at the same boundary.

### Requirement: Failed multi-VM rounds poison the controller

r[chaoscontrol.deterministic_smp.controller_poison] If a VM schedule poison or another error occurs after round mutation starts, the controller MUST permanently latch the failed round before returning.

#### Scenario: A later VM poisons after an earlier VM advances

r[chaoscontrol.deterministic_smp.validation.controller_poison]
- GIVEN VM0 records deterministic progress and VM1 then poisons during the same controller round
- WHEN the caller retries execution or requests a mutation, snapshot, restore, recording, or success result
- THEN the controller MUST reject the request before mutation
- AND VM0 progress, simulation tick, fault state, and network state MUST NOT advance again
- AND partial VM journals MUST remain diagnostic-only
- AND the failed round MUST NOT be published as complete.

#### Scenario: Preflight fails before mutation

- GIVEN a round preflight rejects bounded input before any round mutation
- WHEN the caller corrects that input and retries
- THEN the controller MAY retry without a permanent poison.

### Requirement: Deterministic SMP validation perturbs the host

r[chaoscontrol.deterministic_smp.validation] The change MUST test pure schedule transitions, exact spin-loop preemption, unavailable and overshooting progress sources, spurious interrupts, snapshot boundaries, failed multi-VM rounds, and repeated KVM execution under varied host delay and contention.

#### Scenario: Host timing is perturbed

r[chaoscontrol.deterministic_smp.validation.host_perturbation]
- GIVEN the same guest, configuration, seed, capability profile, and deterministic progress source run with varied host sleeps, watchdog cadence, and CPU contention
- WHEN accepted runs complete
- THEN their canonical vCPU transition traces and bounded guest observations MUST match
- AND any operational timeout MUST be reported separately rather than compared as a deterministic completion.
