# Fault Mechanisms Specification

## Purpose

Extend the deterministic fault surface with directional network faults, CPU throttling, typed custom effects, and bounded pause control while preserving the existing stage ledger, snapshot, replay, and claim boundaries.

## ADDED Requirements

### Requirement: Network faults are directional and link-scoped

r[chaoscontrol.fault_mechanisms.directional_network] A network fault MUST admit a named link target, an `inbound`, `outbound`, or `both` direction, and a `stopped`, `slowed`, or `jammed` disruption mode. A stopped fault MUST drop affected packets, a slowed fault MUST add a declared delay, and a jammed fault MUST queue them for later delivery within a declared bound. A scoped heal MUST clear only the named link. An admitted baseline latency/low-loss profile MUST be explicit, seed-bound, and recorded per affected packet observation. Unsupported links, directions, modes, or out-of-bound queues MUST return typed rejection facts.

#### Scenario: One direction is disrupted

- GIVEN a link between two admitted VMs and an outbound-only slowed fault
- WHEN traffic crosses the link in both directions
- THEN only outbound traffic MUST receive the declared delay
- AND inbound traffic MUST remain under the baseline profile.

#### Scenario: A scoped heal runs

- GIVEN two links with active faults
- WHEN a scoped heal names one link
- THEN only that link MUST be cleared
- AND the other link's active fault MUST remain.

### Requirement: CPU throttling has a deterministic virtual budget

r[chaoscontrol.fault_mechanisms.cpu_throttle] A CPU-throttle fault MUST name a target VM or admitted process, a finite virtual instruction or quantum budget, and a finite release window. The scheduler MUST admit no further target progress after the budget is exhausted and MUST release at the exact window tick. It MUST NOT use host CPU affinity, cgroups, `nice`, priority changes, or wall-clock sleeps. Unsupported target classes or profiles MUST return typed rejections.

#### Scenario: A target exhausts its budget

- GIVEN an admitted target with a finite quantum budget
- WHEN it consumes the budget inside its window
- THEN the scheduler MUST stop admitting target progress until the release tick
- AND other admitted work MUST continue deterministically.

#### Scenario: Host scheduling guidance is requested

- GIVEN a profile asks for a host cgroup or wall-clock throttle
- WHEN the fault is admitted
- THEN it MUST be rejected as unsupported.

### Requirement: Custom faults are typed admitted effects

r[chaoscontrol.fault_mechanisms.custom_effect] A custom fault MUST be a typed descriptor with a stable effect identity, bounded typed parameters, a declared version, and one admitted Rust-owned adapter. Selection and ordering MUST use the same seeded schedule and bounds as catalogue faults. The adapter MUST NOT invoke an ambient shell, unbounded script, dynamic library, network fetch, package installation, or unadmitted executable. Unknown, version-mismatched, over-bound, or failed effects MUST record typed rejection or application failure.

#### Scenario: A declared effect is selected

- GIVEN an admitted custom-effect descriptor and adapter
- WHEN the seeded schedule selects it
- THEN the adapter MUST receive only the declared bounded parameters
- AND the stage ledger MUST retain selection, application, and observation outcomes.

#### Scenario: An adapter requests an ambient command

- GIVEN a custom effect attempts to execute an unadmitted shell or fetch a dependency
- WHEN admission or execution runs
- THEN the effect MUST be rejected
- AND no external process or network action MUST occur.

### Requirement: Pause control suppresses only new fault work

r[chaoscontrol.fault_mechanisms.pause_control] A workload or operator pause-control request MUST suppress new fault selection and application for a finite virtual-time window. The request MUST record its requester, start tick, end tick, and current active effects. Already-applied effects MUST continue unless a separately admitted release mechanism is invoked, and the window MUST NOT rewind or erase an effect. Duplicate, overlapping, malformed, or out-of-horizon requests MUST return typed decisions. Snapshot and replay MUST preserve the window and suppression facts.

#### Scenario: A workload opens a quiet window

- GIVEN a workload requests pause control for a finite virtual window
- WHEN a fault would otherwise be selected
- THEN the selection MUST be suppressed with the pause-window reason
- AND active effects MUST retain their recorded state.

#### Scenario: A pause window overlaps another request

- GIVEN an active pause window and a second request with a different requester
- WHEN the second request is admitted
- THEN the policy MUST reject or merge it deterministically
- AND the receipt MUST retain both request facts.

### Requirement: Overlap and precedence are deterministic

r[chaoscontrol.fault_mechanisms.interactions] When network faults overlap on one link, the more restrictive admitted mode MUST govern the overlap while every fault window remains recorded. Process and resource faults MUST admit at most one active effect per target; a later request MUST be rejected or skipped with a stable reason. Pause control MUST affect only new selection and application. The receipt MUST identify the effective fault on each affected link or target and every suppressed, skipped, or rejected request.

#### Scenario: Two network faults overlap

- GIVEN a slowed fault and a stopped fault on the same directional link
- WHEN their windows overlap
- THEN the stopped mode MUST govern the overlap
- AND both windows MUST remain visible in evidence.

#### Scenario: Two process faults target one process

- GIVEN an active process kill or pause on a target
- WHEN another process fault targets it
- THEN the later request MUST be rejected or skipped with a stable reason
- AND no second effect MUST be applied.

### Requirement: Every mechanism uses the stage ledger

r[chaoscontrol.fault_mechanisms.evidence] Each new mechanism MUST record `Selected`, `Applicable`, `Applied`, `Observed`, `Rejected`, or `ApplicationFailed` with the exact plan, bounds, effective target, and observed outcome. The receipt MUST bind the profile, seed, mechanism identities, suppression windows, overlap decisions, and per-mechanism counts while excluding raw logs and payloads beyond declared bounds. Selection MUST NOT be reported as application or observation.

#### Scenario: A fault is selected but not observed

- GIVEN a custom effect or directional fault is applied but no affected traffic or execution reaches it
- WHEN the run receipt is written
- THEN it MUST show application without observation
- AND it MUST NOT claim the workload was affected.

### Requirement: New fault mechanisms keep bounded claims

r[chaoscontrol.fault_mechanisms.nonclaims] A fault result MUST NOT claim host-level fault injection, universal hardware behavior, guaranteed failure discovery, Antithesis parity, correctness, complete fault coverage, replay promotion, or release eligibility. It MUST describe only the admitted mechanism, target, window, observed stage, and recorded run facts.

#### Scenario: A mechanism is promoted to coverage proof

- GIVEN a receipt with one observed custom effect
- WHEN a report claims the mechanism covers all custom failure modes
- THEN non-claim validation MUST reject the report.

### Requirement: The expanded fault surface has positive and negative fixtures

r[chaoscontrol.fault_mechanisms.fixtures] The rail MUST include positive fixtures for directional traffic, jammed delivery, scoped heal, quantum exhaustion and release, a typed custom effect, pause-window suppression, active-effect continuation, and overlap precedence, plus negative fixtures for unsupported direction, over-bound queue, host-throttle request, ambient command, unadmitted adapter, duplicate process effect, malformed pause window, snapshot drift, replay divergence, and overclaim.

#### Scenario: The rail is proposed for product use

- GIVEN the mechanisms, planner, scheduler, network fabric, adapters, evidence, and fixtures are complete
- WHEN focused Cargo, octet, Cairn, and Nix checks run
- THEN every positive fixture MUST pass
- AND every negative fixture MUST fail at its declared boundary.
