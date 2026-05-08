# Disk Advanced Faults Specification

## Purpose

Defines the canonical ChaosControl requirements for disk advanced faults.

## Requirements
### Requirement: DiskSlow fault adds latency to block I/O

This requirement MUST be satisfied by the corresponding ChaosControl implementation and validation evidence.
The fault engine SHALL support a `DiskSlow` fault variant that injects a
per-operation delay into a target VM's block device reads and writes. The
delay is specified in nanoseconds of virtual time and persists until cleared
by a new `DiskSlow` with `delay_ns: 0` or by removing the fault state.

#### Scenario: Slow reads
- **WHEN** a `DiskSlow { target: 0, delay_ns: 5_000_000 }` fault is active
- **THEN** every block read on VM 0 SHALL advance the virtual TSC by an additional 5 ms worth of TSC ticks before returning data

#### Scenario: Slow writes
- **WHEN** a `DiskSlow { target: 0, delay_ns: 10_000_000 }` fault is active
- **THEN** every block write on VM 0 SHALL advance the virtual TSC by an additional 10 ms worth of TSC ticks before the write completes

#### Scenario: Clear slow I/O
- **WHEN** a `DiskSlow { target: 0, delay_ns: 0 }` fault fires
- **THEN** VM 0's block device SHALL return to normal latency (no additional delay)

#### Scenario: Snapshot/restore preserves slow state
- **WHEN** a VM snapshot is taken while `DiskSlow` is active
- **AND** the snapshot is restored
- **THEN** the slow I/O delay SHALL still be in effect after restore

### Requirement: DiskFsyncLie fault silently drops unflushed writes

This requirement MUST be satisfied by the corresponding ChaosControl implementation and validation evidence.
The fault engine SHALL support a `DiskFsyncLie` fault variant that models
power-loss data loss on filesystems with writeback caching. When active,
writes go to a volatile buffer instead of the durable CoW store. A subsequent
`DiskFsyncFlush` fault (or deactivation) commits the volatile buffer to
durable storage. If the VM is killed (ProcessKill) while `DiskFsyncLie` is
active, all volatile writes since the last flush SHALL be discarded.

#### Scenario: Writes accumulate in volatile buffer
- **WHEN** `DiskFsyncLie { target: 0 }` is active
- **AND** VM 0 performs block writes
- **THEN** the writes SHALL be visible to subsequent reads (volatile buffer is read-through)
- **AND** the writes SHALL NOT be persisted to the durable CoW store

#### Scenario: Kill discards volatile writes
- **WHEN** `DiskFsyncLie { target: 0 }` is active
- **AND** VM 0 performs block writes
- **AND** `ProcessKill { target: 0 }` fires before any flush
- **AND** VM 0 is restarted
- **THEN** the block device SHALL NOT contain the volatile writes

#### Scenario: Flush commits volatile buffer
- **WHEN** `DiskFsyncLie { target: 0 }` is active
- **AND** VM 0 performs block writes
- **AND** `DiskFsyncFlush { target: 0 }` fires
- **THEN** all accumulated volatile writes SHALL be committed to the durable CoW store
- **AND** subsequent kills SHALL NOT discard the flushed data

#### Scenario: Snapshot captures volatile buffer
- **WHEN** `DiskFsyncLie` is active with pending volatile writes
- **AND** a snapshot is taken
- **THEN** the snapshot SHALL include both the durable state and the volatile buffer
- **AND** restore SHALL reproduce the exact same volatile/durable split

### Requirement: DiskPartialRead returns fewer bytes than requested

This requirement MUST be satisfied by the corresponding ChaosControl implementation and validation evidence.
The fault engine SHALL support a `DiskPartialRead` fault variant that
causes a target VM's next read at a specific offset to return fewer bytes
than the buffer size. This models degraded storage returning short reads.

#### Scenario: Short read triggered
- **WHEN** `DiskPartialRead { target: 0, offset: 4096, max_bytes: 256 }` is queued
- **AND** VM 0 reads 512 bytes at offset 4096
- **THEN** only the first 256 bytes of the buffer SHALL contain valid data
- **AND** the fault SHALL be consumed (one-shot)

#### Scenario: Fault is one-shot
- **WHEN** a `DiskPartialRead` fault fires on a read
- **THEN** subsequent reads at the same offset SHALL return full data (fault consumed)

### Requirement: Random generation includes new disk faults

This requirement MUST be satisfied by the corresponding ChaosControl implementation and validation evidence.
The FaultEngine's `generate_random_fault()` and the ScheduleMutator's
`random_fault()` SHALL include `DiskSlow`, `DiskFsyncLie`, and
`DiskPartialRead` in their random selection pool with reasonable parameter
ranges.

#### Scenario: Random fault pool includes new types
- **WHEN** generating 1000 random faults
- **THEN** at least one `DiskSlow`, one `DiskFsyncLie`, and one `DiskPartialRead` SHALL appear

### Requirement: Serialization backward compatibility

This requirement MUST be satisfied by the corresponding ChaosControl implementation and validation evidence.
New fault variants SHALL serialize/deserialize via serde without breaking
existing checkpoint or bug report JSON files. Unknown variants in old files
SHALL be skipped or cause a clear error, not a silent data loss.

#### Scenario: Old checkpoint loads without new faults
- **WHEN** loading a checkpoint JSON that predates the new fault variants
- **THEN** deserialization SHALL succeed with no errors

#### Scenario: New checkpoint roundtrips
- **WHEN** a checkpoint containing `DiskSlow`, `DiskFsyncLie`, or `DiskPartialRead` faults is saved
- **AND** the checkpoint is loaded
- **THEN** the fault schedule SHALL be identical to the original
