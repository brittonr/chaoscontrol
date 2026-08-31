# Guest Multiprocess Topology Specification

## Purpose

Defines the `guest-multiprocess-topology` capability.

## Requirements

### Requirement: Guest images declare processes

r[chaoscontrol.guest_processes.manifest] A guest image MAY declare zero or more processes. Each process MUST bind an executable path, role name, arguments, and shared working directory membership.

#### Scenario: Single-process image
- GIVEN a guest image with no process declarations
- WHEN the image boots
- THEN the legacy single-process init path MUST behave without change.

#### Scenario: Manifest with cooperating processes
- GIVEN a manifest declaring a writer process and a checkpoint process in one shared working directory
- WHEN the supervisor starts the guest
- THEN both processes MUST run and share the admitted storage surface.

### Requirement: The supervisor owns process lifetime

r[chaoscontrol.guest_processes.supervisor] When a guest declares processes, a deterministic supervisor MUST spawn, monitor, and restart them under an explicit process policy, and MUST emit observable lifecycle events.

#### Scenario: Process exits
- GIVEN one declared process exits
- WHEN the policy requires restart
- THEN the supervisor MUST restart that process and record the restart deterministically.

### Requirement: Shared storage is one admitted device

r[chaoscontrol.guest_processes.shared_storage] Processes declared in one shared working directory MUST observe one deterministic block or memory device mounted at that directory.

#### Scenario: Shared writes are visible across processes
- GIVEN two processes in one shared working directory
- WHEN one process writes a file through the admitted device
- THEN the other process MUST observe that file.

#### Scenario: Restart preserves shared state
- GIVEN one process restarts
- WHEN the shared working directory is checked
- THEN device-backed state MUST survive the restart.

### Requirement: Process faults target a process identity

r[chaoscontrol.guest_processes.process_faults] A process fault MUST name a process role or identity, and MUST kill, pause, or restart only that process.

#### Scenario: One process crashes
- GIVEN a three-process guest and a crash fault on one role
- WHEN the fault fires
- THEN the other two processes MUST keep running.

#### Scenario: Invalid target
- GIVEN a process fault naming an unknown role
- WHEN the fault is applied
- THEN the fault MUST record a typed rejection.

### Requirement: Per-process SDK isolation

r[chaoscontrol.guest_processes.transport_isolation] Two SDK-instrumented processes in one guest MUST emit assertion and lifecycle events through isolated transports without cross-process corruption.

#### Scenario: Concurrent assertion traffic
- GIVEN two processes emitting assertions at the same virtual time
- WHEN the property oracle aggregates the events
- THEN every assertion MUST bind to its owning process identity.

#### Scenario: Uninstrumented process
- GIVEN a declared process not linked against the SDK
- WHEN the process runs
- THEN it MUST run normally and emit no assertion traffic.

### Requirement: Multiprocess evidence is process-scoped

r[chaoscontrol.guest_processes.evidence_scope] Assertion records, bug reports, and replay verdicts MUST record the owning process identity, and MUST NOT promote a process-local fact to a whole-guest claim.

#### Scenario: Failure attribution
- GIVEN an assertion failure in one process
- WHEN the bug report is produced
- THEN the report MUST name the process role and identity.

#### Scenario: Green multiprocess run
- GIVEN a guest with multiple processes and no assertion failure
- WHEN the campaign reports
- THEN the report MUST state that the evidence covers the declared processes only.

### Requirement: Multiprocess validation is adversarial

r[chaoscontrol.guest_processes.validation] Validation MUST pair a positive cooperating-process fixture with negative fixtures for process crash survival, restart, invalid fault target, transport corruption, and shared-state loss.

#### Scenario: Closeout validation runs
- GIVEN maintainers intend to admit multiprocess guests
- WHEN pure, VM, replay, and lifecycle validation runs
- THEN every positive and negative class MUST produce its expected result.
