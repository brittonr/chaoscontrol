# Incremental Snapshots Specification

## Purpose

Defines the canonical ChaosControl requirements for incremental snapshots.

## Requirements
### Requirement: Dirty page tracking via KVM
The VMM SHALL enable KVM dirty page logging on the guest memory slot
when incremental snapshots are active. The dirty bitmap SHALL be
retrieved via `KVM_GET_DIRTY_LOG` and reset atomically so that
subsequent queries reflect only new writes.

#### Scenario: Enable dirty tracking after bootstrap
- **WHEN** the explorer completes bootstrap and takes the initial snapshot
- **THEN** dirty logging is enabled on the guest memory region and subsequent
  `KVM_GET_DIRTY_LOG` calls return only pages written since the last query

#### Scenario: Dirty bitmap accuracy
- **WHEN** a branch writes to N guest pages during 1000 ticks of execution
- **THEN** `KVM_GET_DIRTY_LOG` returns a bitmap with exactly those N pages
  marked (plus any pages dirtied by KVM internal state, such as APIC MMIO)

### Requirement: Overlay snapshot format
Snapshots SHALL store guest memory as a shared immutable base
(`Arc<[u8]>`) plus a sparse overlay of dirty pages. The overlay SHALL
use 4 KB page granularity. Cloning a snapshot SHALL clone only the
overlay, sharing the base via reference count.

#### Scenario: Snapshot size proportional to dirty pages
- **WHEN** a branch dirties 1 MB of a 256 MB guest
- **THEN** the overlay snapshot consumes approximately 1 MB plus per-page
  index overhead, not 256 MB

#### Scenario: Clone cost proportional to overlay
- **WHEN** a snapshot with a 500-page overlay is cloned
- **THEN** the clone operation copies only the overlay map entries
  and increments the base reference count

### Requirement: Incremental snapshot capture
`VmSnapshot::capture` SHALL accept an optional dirty bitmap. When
provided, it SHALL copy only the pages marked dirty from guest memory
into the overlay. When no bitmap is provided (initial snapshot), it
SHALL copy all pages into a new base.

#### Scenario: Initial snapshot captures full memory
- **WHEN** `snapshot()` is called without a prior dirty bitmap
- **THEN** all guest memory is copied into the base allocation and
  the overlay is empty

#### Scenario: Incremental snapshot captures only dirty pages
- **WHEN** `snapshot()` is called with a dirty bitmap marking 200 pages
- **THEN** only those 200 pages are read from guest memory and stored
  in the overlay

### Requirement: Incremental restore
`VmSnapshot::restore` SHALL write only the overlay pages back to guest
memory. Pages not in the overlay are already correct in guest memory
(unchanged from the base or a prior restore). A full restore from a
fresh base SHALL write all pages.

#### Scenario: Restore writes only overlay pages
- **WHEN** a snapshot with a 300-page overlay is restored
- **THEN** only 300 × 4 KB = 1.2 MB is written to guest memory, not
  256 MB

#### Scenario: Full restore from base
- **WHEN** a snapshot with no base sharing (initial snapshot) is restored
- **THEN** all guest memory is overwritten from the snapshot

### Requirement: Flatten overlay to full snapshot
An overlay snapshot SHALL support materialization into a contiguous
byte vector for serialization (checkpoints, bug reports). This
operation applies the overlay on top of the base.

#### Scenario: Materialize for checkpoint
- **WHEN** a checkpoint is saved with an overlay snapshot
- **THEN** the materialized memory equals the base with overlay pages
  applied on top, byte-for-byte identical to what a full snapshot would
  have produced

### Requirement: Backward compatibility
The full-copy snapshot path SHALL remain available as a fallback. VMs
that do not enable dirty tracking SHALL continue to produce and consume
full snapshots.

#### Scenario: Legacy snapshot still works
- **WHEN** dirty tracking is not enabled
- **THEN** snapshot and restore behave identically to the current
  full-copy implementation
