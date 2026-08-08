## Context

`DeterministicVm::new` creates an `InstructionCounter::new()` for SMP and describes PMU-plus-single-step quantum enforcement, yet `insn_count` and `switch_vcpu_at_quantum` are unused in the execution path. `step` instead arms a host timer and changes `active_vcpu` after a fixed number of `VcpuExit::Intr` or `EINTR` results. Real exits during those timer-selected detours advance global exit counters and virtual time even though the scheduler does not record the detour.

The implementation needs one explicit authority for scheduling decisions and a separate host-liveness shell.

## Decisions

### 1. Use a pure schedule transition core

**Choice:** Define immutable schedule inputs and an explicit `ScheduleState` containing active vCPU, runnable set, policy state, seeded-choice state, per-vCPU deterministic progress, quantum boundary, and pending exact-step state. A pure transition function consumes typed guest events and returns the next state plus an auditable action such as continue, switch, halt, or reject.

Only replay-stable guest exits, exact progress events, explicit guest yields, deterministic interrupt delivery, and seeded policy choices are valid transition inputs. Host signals, elapsed time, thread identity, and host scheduling are not.

**Rationale:** Centralizing selection prevents an operational interrupt branch from mutating guest scheduling behind the scheduler's accounting.

### 2. Require an exact no-exit progress source

**Choice:** The deterministic SMP profile provides at least one exact progress source capable of reaching a named guest-instruction boundary even when the guest produces no ordinary VM exits. A portable all-single-step mode is an acceptable correctness baseline. A PMU-accelerated mode may run to a guarded boundary and single-step the remainder only when capability checks and runtime counters establish that it did not overshoot.

If an accelerated source overshoots, loses an event, or reports inconsistent progress, the run terminates with a deterministic-progress error rather than switching at the observed host time.

**Rationale:** Exit-count scheduling alone cannot preempt a spin loop, while host-time preemption does not define a replayable instruction boundary.

### 3. Fail closed instead of using timer-only fallback

**Choice:** VM creation selects a declared deterministic progress mode and validates its required KVM/PMU/debug capabilities before guest execution. An unavailable mode returns a typed capability failure or uses another explicitly selected deterministic mode. There is no implicit `SIGALRM`-only SMP mode under a deterministic configuration.

**Rationale:** Logging a fallback does not prevent downstream tooling from treating the run as deterministic.

### 4. Treat wall-clock interruption as shell-owned watchdog state

**Choice:** POSIX timers may interrupt a blocked `KVM_RUN` only to regain host control. Their handler and `EINTR` path may retry or end the operation with a `HostWatchdogTimeout` classification, but they do not advance virtual time, increment deterministic progress, switch vCPUs, or set guest panic/crash state.

Watchdog timeout facts are operational diagnostics and are excluded from deterministic replay acceptance. A replayable guest-time timeout must be modeled through deterministic virtual progress instead.

**Rationale:** This preserves host recoverability without laundering wall-clock observations into guest behavior.

### 5. Make progress and switch evidence explicit

**Choice:** Each schedule transition records the declared progress source, pre-transition state identity, deterministic boundary/event, selected action, and post-transition state identity. State identities use BLAKE3 over canonical schedule state. Timer arrival timestamps and signal counts are not part of that identity.

The schedule state is snapshot-ready and restored by the VM snapshot owner; this package defines its semantics while `complete-vm-snapshot-state` owns whole-VM inventory and restore ordering.

The VM permanently poisons execution after any post-entry evidence failure. It also poisons execution if exit handling or schedule-action application fails after the journal commit. A committed partial journal remains diagnostic-only.

**Rationale:** Existing dlog interrupt records show that an interrupt occurred but do not prove why a vCPU changed.

### 6. Validate against host perturbation

**Choice:** Pure tests exhaust small schedule states and event sequences. Integration tests replay a spin-looping SMP fixture with injected spurious `EINTR`, varied watchdog cadence, host sleeps, CPU contention, and PMU-unavailable profiles. Accepted runs must produce identical vCPU transition traces and bounded guest observations; unavailable exact modes must fail before execution.

**Rationale:** Repeating on an idle host alone does not exercise the source of the defect.

### 7. Permanently poison a failed controller round

**Choice:** The controller latches the first error after a round starts mutation. The latch records the round, starting tick, and original failure.

All later execution, controller mutation, snapshot, restore, recording, and success-result paths fail before mutation. Partial VM journals remain diagnostic data only. The controller never emits a complete result for the failed round.

Preflight failures remain retryable only when no round mutation occurred.

**Rationale:** One VM can advance before a later VM loses exact evidence. Retrying that round would advance the earlier VM twice and corrupt global fault, network, or tick state.

## Risks / Trade-offs

- Full single stepping can be expensive. Performance cannot justify an undeclared non-deterministic fallback.
- Hardware instruction counters have host-specific semantics. Accelerated mode must bind its capability profile and detect boundary violations rather than claim universal portability.
- A host watchdog timeout is useful operational evidence but intentionally cannot prove a guest deadlock or crash.
- Schedule-state format changes must coordinate with the complete snapshot package without taking ownership of replay artifact DTOs.
