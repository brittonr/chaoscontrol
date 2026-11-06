# KVM Debug-Boundary Scheduling Specification

## Purpose

Define a separately selected coarse KVM debug/exit-boundary capability for first-party Rust/Linux SMP campaigns and persisted replay/debugging without weakening strict retired-instruction scheduling. This delta is a plan, not accepted implementation or runtime evidence.

## ADDED Requirements

### Requirement: Boundary units and atomic regions are explicit

r[chaoscontrol.kvm_debug_boundary.units] Coarse execution MUST declare `KvmDebugBoundary` mode, boundary units and an immutable versioned boundary-profile/table identity. One unit MUST mean one validated, settled, committed guest-origin boundary from that profile's closed exit table, never one retired instruction, host interruption, KVM entry, injection intention or elapsed interval. Interrupt-handler entry, body, return and other invisible work between accepted boundaries MUST form a scheduler-atomic region; a handler's own accepted exit MAY split that region. The system MUST NOT infer hidden instruction counts, delivery points or IRET completion from a debug return.

#### Scenario: IRQ handler suppresses stepping

- GIVEN IRQ entry hides multiple handler instructions before the next accepted debug exit
- WHEN the shell commits that boundary
- THEN it MUST count one boundary unit, MUST NOT call it one retired instruction, and MUST NOT schedule another vCPU or expose a snapshot/debugger position inside the hidden interval.

#### Scenario: Guest I/O needs completion re-entry

- GIVEN an allowed exit requires pending I/O settlement before a safe continuation point
- WHEN the shell completes that operation
- THEN the versioned profile MUST define its single boundary and completion classification, MUST NOT double-count settlement re-entry, and MUST reject ambiguous or incomplete state rather than publish a safe checkpoint.

#### Scenario: An exit is outside the declared table

- GIVEN an unknown exit reason or unexpected debug cause is returned after possible guest execution
- WHEN the shell classifies it
- THEN it MUST reject the boundary and poison the attempted execution without manufacturing progress.

### Requirement: Selection is separate from strict admission

r[chaoscontrol.kvm_debug_boundary.admission] Strict `ExactSingleStep` and `PmuAccelerated` modes MUST retain retired-instruction semantics and fail-closed admission wherever required exact capabilities are unsupported. Coarse execution MUST require explicit `--smp-progress-mode kvm-debug-boundary` selection and a matching declared boundary budget/profile, including explicit opt-in when replaying a retained coarse session. Missing selection, unknown modes, mixed units, incompatible capabilities or configuration MUST reject before guest execution. No strict failure, old artifact, or CLI default MAY trigger coarse fallback; coarse claims MUST NOT satisfy strict instruction proof obligations.

#### Scenario: Current KVM cannot establish strict IRQ-entry semantics

- GIVEN a strict request cannot establish exact retired-instruction boundaries across supported delivery paths
- WHEN admission evaluates the request
- THEN strict mode MUST reject with its typed capability reason even if a separate coarse mode could run.

#### Scenario: Operator explicitly selects the coarse profile

- GIVEN an independently admitted coarse configuration and explicit mode/boundary-quantum flags
- WHEN the first-party Rust/Linux SMP campaign starts
- THEN the execution descriptor and all progress output MUST name boundary units and MUST NOT render the run as strict instruction execution.

#### Scenario: Persisted coarse input lacks operator opt-in

- GIVEN a session declares a coarse mode but the replay or inspect invocation has no matching explicit coarse selection
- WHEN execution intake runs
- THEN it MUST reject rather than treat the manifest as an implicit downgrade authorization.

### Requirement: Existing policy core is reused with sound typed semantics

r[chaoscontrol.kvm_debug_boundary.scheduler] One pure policy owner MUST select the vCPU using explicit runnable state, typed turn budgets, progress observations and seeded policy state. The cutover MUST migrate `VmConfig::smp_instruction_quantum`, `SchedulerConfig::quantum`, `ScheduleVariant::quantum_override`, instruction-only schedule state, `GuestProgress`, and remaining/exact-step APIs to the discriminated instruction-versus-boundary APIs specified in the design. Boundaries MUST NOT flow through `ProgressSource::ExactSingleStep` or PMU/exact-step remainder arithmetic. Reuse MUST preserve journal preflight/commit, checked arithmetic, stale-state validation, policy RNG and poison rather than introduce a second scheduler. Every caller and behavioral fixture MUST migrate without old-name aliases, compatibility re-exports or duplicate continuation loops.

#### Scenario: Identical boundary observations reach policy

- GIVEN identical typed budgets, profile, seed/RNG position, runnable set and accepted boundary events
- WHEN the pure core plans transitions
- THEN it MUST produce equal selections and canonical identities without KVM, clocks, signals, filesystem or thread access.

#### Scenario: Boundary count is passed as instruction progress

- GIVEN a boundary-mode state receives an instruction/PMU observation or a mixed-unit override
- WHEN the core validates the input
- THEN it MUST reject without mutating state rather than convert the count.

### Requirement: Finite watchdog recovery has abort authority only

r[chaoscontrol.kvm_debug_boundary.recovery] Every coarse KVM entry, including SMP hidden-handler execution, MUST have named finite host-control and outer kill/reap bounds with actual recovery capability admitted before execution. Host time, signals, `Intr`/`EINTR`, watchdog cadence and host scheduling MUST NOT advance progress or virtual time, choose a vCPU, inject a guest event, or publish a target. Unverified possible partial execution or a post-entry evidence/exit/action failure MUST permanently poison the VM and any partially mutated controller round. No completed round, verified position, usable snapshot/restore or successful trace drain MAY escape poison; later execution requires a fresh runner and independently admitted checkpoint.

#### Scenario: Handler never produces another boundary

- GIVEN TF is suppressed inside a no-exit interrupt-handler loop while another vCPU is runnable
- WHEN the finite host recovery deadline expires
- THEN control MUST recover or the finite supervisor MUST terminate/reap the worker, the result MUST remain an operational timeout, and no fabricated quantum switch, guest deadlock verdict or completed session MAY be emitted.

#### Scenario: Host interruption leaves guest work unverified

- GIVEN a host signal interrupts execution after guest state may have changed without a complete verifiable boundary
- WHEN the shell handles that return
- THEN it MUST permanently poison and abort that runner rather than release its reservation and retry as though no guest work happened.

#### Scenario: Host event is interleaved with verified observations

- GIVEN a fixed sequence of already-validated guest boundaries and unrelated host events
- WHEN pure schedule validation evaluates them
- THEN host events MUST have no deterministic authority and MUST NOT change the canonical accepted schedule; this fixture alone MUST NOT count as real KVM delivery evidence.

### Requirement: Evidence and persistence bind mode and delivery limits

r[chaoscontrol.kvm_debug_boundary.persistence] Canonical traces, snapshots, recording/session configuration and claim identities MUST bind mode, typed units, boundary-profile/table and schema identity, normalized boundary observations, policy/RNG/runnable progress and available queued/pending interrupt/exception/device state and ledger prefix. Requested injection, pending acceptance and actual observable delivery MUST remain distinct; unobservable delivery instruction/order MUST NOT be invented. Unsupported or uncapturable delivery/clock sources MUST fail admission or remain explicitly nonqualified. Snapshot/restore and replay MUST reject unsettled boundaries, mode/unit/profile/schema mismatch and incomplete required state before mutation where possible. Replay and debugger MUST share full actual continuation comparison, publish only a verified declared-coordinate target, and reject hidden-region targets. New domains/schema versions MUST NOT reinterpret old strict artifacts or silently promote coarse results through strict verdict consumers.

#### Scenario: Pending IRQ is restored in a fresh process

- GIVEN a fully settled snapshot with supported pending IRQ/device and delivery-ledger state and a matching coarse execution descriptor
- WHEN a separate replay process restores and continues it
- THEN mode, units, profile and full captured pending state MUST match before execution and actual canonical continuation MUST be compared against the recorded control.

#### Scenario: Delivery is not fully observable

- GIVEN injection is requested at a recorded boundary but KVM hides its precise delivery point or exception ordering
- WHEN evidence is serialized
- THEN it MUST retain only known request/pending/delivery facts and explicit uncertainty, MUST NOT claim instruction-level delivery determinism, and MUST reject any unsupported profile prerequisite.

#### Scenario: Coarse schema or trace is corrupt

- GIVEN a retained artifact has altered units/profile, missing snapshot, forged/reordered/omitted boundaries or injection records, mismatched identity or incomplete pending I/O
- WHEN replay or inspect admits it
- THEN both MUST reject and MUST NOT supply defaults, convert strict artifacts, boot instead of restore or expose an unverified position.

### Requirement: Actual repeated KVM controls precede bounded claims

r[chaoscontrol.kvm_debug_boundary.evidence] Qualification MUST include an actual bounded first-party Rust/Linux SMP campaign and repeated replay controls with complete canonical schedule/delivery/fault and declared guest-observation equality; interrupt/POPF/IRET/NMI/exception and pending-I/O probes; a nonreturning no-exit handler; host-signal/delay/contention/watchdog perturbation; permanent-poison and partial-round failures; full producer-exit/fresh-process persisted replay-and-inspect reload; and paired corruption/mode/unit/profile/opt-in controls. Every outcome, including operational failures and unsupported delivery classes, MUST be retained under named finite bounds and exact source/config/host/artifact identities. Only matched real repeated controls MAY support a bounded same-cohort boundary-replay claim. Planning/gate passes, pure fixtures, one successful boot, requested injection, or raw logs MUST NOT establish determinism, instruction equivalence, universal IRQ/NMI/exception behavior, strict replay readiness or cross-host correctness.

#### Scenario: Repeated campaign and persisted reload agree

- GIVEN a retained non-default coarse SMP cohort with all required positive and negative controls and named repetition/horizon bounds
- WHEN independent campaign/replay invocations and fresh replay/inspect processes navigate across persisted checkpoints
- THEN full canonical continuations and selected assertions/memory/register/output MUST match the control before any bounded same-profile replay claim, with that claim explicitly qualified by mode, host, delivery classes and horizon.

#### Scenario: A required control diverges or cannot run

- GIVEN an interrupt/signal/reload/corruption/recovery control fails, diverges or lacks its required artifact/capability
- WHEN evidence is reviewed
- THEN qualification MUST remain failed or blocked with the exact gap, and the implementation/evidence/archive tasks MUST stay open rather than use document gates or narrower passing cases as substitutes.
