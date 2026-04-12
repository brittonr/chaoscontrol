## ADDED Requirements

### Requirement: Volatile-write-ring scenario stresses unflushed durability loss
The built-in helical scenario set SHALL include a `volatile-write-ring` family that rotates unflushed-write loss conditions across VMs. For each turn, the scenario SHALL enable `DiskFsyncLie` on the current target, allow a write window, force a crash or isolation before durability is restored, then restart or heal the target before rotating to the next VM.

#### Scenario: One turn injects unflushed-loss sequence
- **WHEN** a `volatile-write-ring` turn is materialized for target VM 1
- **THEN** the concrete schedule includes `DiskFsyncLie { target: 1 }`
- **AND** it later includes a destructive boundary such as `ProcessKill { target: 1 }` or a majority-isolating partition before durability is restored
- **AND** it includes a recovery step before the next turn begins

#### Scenario: Later turns rotate to the next VM
- **WHEN** `volatile-write-ring` runs for multiple turns on 3 VMs
- **THEN** the unflushed-loss sequence is applied to a different VM on each successive turn until the ring wraps

### Requirement: Degraded-io-ring scenario stresses slow and partial I/O recovery
The built-in helical scenario set SHALL include a `degraded-io-ring` family that rotates degraded storage conditions across VMs using `DiskSlow` or `DiskPartialRead`, combined with restart or partition pressure and a later recovery window.

#### Scenario: Degraded I/O turn injects slow or short-read fault
- **WHEN** a `degraded-io-ring` turn is materialized
- **THEN** the concrete schedule includes either `DiskSlow` or `DiskPartialRead` for the current target VM
- **AND** it includes at least one additional recovery-stressing event such as `ProcessRestart` or a partition affecting that VM

#### Scenario: Recovery window follows degraded I/O
- **WHEN** a degraded I/O turn finishes
- **THEN** the next phase includes a window with no new destructive disk faults for the same target VM so recovery assertions can be evaluated

### Requirement: Destructive turns are separated by explicit recovery windows
Helical storage scenarios SHALL insert a bounded recovery window between destructive turns so workloads can observe both failure and recovery behavior instead of stacking destructive events without pause.

#### Scenario: Recovery window has no new destructive storage fault
- **WHEN** a helical storage scenario moves from one target VM to the next
- **THEN** there is an intervening phase of at least one configured phase window with no new `DiskFsyncLie`, `DiskSlow`, or `DiskPartialRead` introduced for the previous target VM
