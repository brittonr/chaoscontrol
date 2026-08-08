# Determinism Log Specification

## Purpose

Defines the canonical ChaosControl requirements for determinism log.

## Requirements
### Requirement: Event logging coverage
The system SHALL log all significant events that affect guest execution including VM exits, RNG draws, fault dispatches, SDK hypercalls, and scheduler decisions. Each event MUST include a monotonic sequence number, virtual TSC, and exit count.

#### Scenario: VM exit logged
- **WHEN** a VM exit occurs during run_bounded with paranoid logging enabled
- **THEN** the log contains an ExitEvent entry with exit type, TSC value, and exit count

#### Scenario: RNG draw logged
- **WHEN** the fault engine or scheduler draws from the RNG
- **THEN** the log contains an RngDraw entry with the domain identifier and drawn value

#### Scenario: SDK hypercall logged
- **WHEN** the guest issues a SDK hypercall (assertion, random_choice, setup_complete)
- **THEN** the log contains an SdkCall entry with the command ID and payload hash

#### Scenario: Scheduler decision logged
- **WHEN** the scheduler switches the active vCPU or the active VM
- **THEN** the log contains a SchedulerDecision entry with the selected index and reason

### Requirement: Binary log format
The system SHALL use a compact binary log format with fixed-size event records for throughput exceeding 1M events per second. The format MUST be self-describing with a file header containing version, event size, and VM metadata.

#### Scenario: Log file structure
- **WHEN** a paranoid log file is opened
- **THEN** it begins with a DlogHeader containing magic bytes, format version, event record size, and VM config hash

#### Scenario: Fixed-size records
- **WHEN** events are written to the log
- **THEN** each record occupies exactly the same number of bytes regardless of event type

### Requirement: Per-VM log streams
The system SHALL maintain separate log streams for each VM instance. Each stream MUST have its own ring buffer and output file named `vm-{id}.dlog`.

#### Scenario: Multi-VM logging
- **WHEN** a 3-VM simulation runs with paranoid logging enabled
- **THEN** three separate log files are created: vm-0.dlog, vm-1.dlog, vm-2.dlog
- **AND** events from VM 0 appear only in vm-0.dlog

### Requirement: Deterministic diff tool
The system SHALL provide a diff subcommand that reads two log files, finds the first divergence, and prints surrounding context. The diff MUST compare event type, TSC, exit count, and payload.

#### Scenario: Identical runs produce no diff
- **WHEN** two logs from identical seeds are compared
- **THEN** the diff tool reports zero divergences

#### Scenario: Divergent runs show first divergence
- **WHEN** two logs from different seeds are compared
- **THEN** the diff tool identifies the first event where values differ
- **AND** prints 10 events of context before and after the divergence point

### Requirement: Configuration control
The system SHALL allow enabling paranoid logging via `VmConfig.paranoid_log: Option<PathBuf>` or CLI `--paranoid-log <dir>`. Logging MUST be disabled by default with zero overhead when disabled.

#### Scenario: Enabled via CLI
- **WHEN** `chaoscontrol-explore run --paranoid-log ./logs` is invoked
- **THEN** log files are created in `./logs/` and events are recorded

#### Scenario: Disabled by default
- **WHEN** no paranoid-log flag is set
- **THEN** no log files are created and no logging overhead is incurred

### Requirement: Ring buffer with async flush
The system SHALL use an in-memory ring buffer per VM that flushes to disk asynchronously. The ring buffer MUST NOT drop events under normal load. A flush MUST be triggered when the buffer reaches 75% capacity or when the simulation ends.

#### Scenario: Buffer flush on capacity
- **WHEN** the ring buffer reaches 75% full during a high-exit-rate workload
- **THEN** an async flush writes buffered events to disk without blocking run_bounded

#### Scenario: Final flush on simulation end
- **WHEN** the simulation completes or the VM is dropped
- **THEN** all remaining buffered events are flushed to the log file
