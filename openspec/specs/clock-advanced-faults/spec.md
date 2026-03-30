## ADDED Requirements

### Requirement: ClockFreeze fault holds TSC at a fixed value
The fault engine SHALL support a `ClockFreeze` fault variant that freezes
a target VM's virtual TSC at its current value for `duration_ticks` ticks.
During the freeze, `sync_tsc_to_guest()` SHALL write the frozen value
instead of the advancing virtual TSC. When the freeze expires, the TSC
resumes from where it was frozen (no jump — the VM experiences a time gap
in wall-clock terms but zero TSC progress).

#### Scenario: TSC frozen during active period
- **WHEN** `ClockFreeze { target: 0, duration_ticks: 100 }` fires at virtual TSC = T
- **THEN** for the next 100 ticks, VM 0's TSC SHALL read as T on every `rdtsc`
- **AND** after tick 100, the TSC SHALL resume advancing from T

#### Scenario: Guest perceives stalled time
- **WHEN** a clock freeze is active
- **AND** the guest reads `rdtsc` twice across exits
- **THEN** both reads SHALL return the same value

#### Scenario: Freeze expires automatically
- **WHEN** the freeze duration elapses
- **THEN** normal TSC advancement SHALL resume with no explicit thaw fault needed

#### Scenario: Snapshot preserves freeze state
- **WHEN** a snapshot is taken during an active freeze
- **AND** the snapshot is restored
- **THEN** the remaining freeze duration and frozen TSC value SHALL be preserved

### Requirement: ClockJitter fault adds per-exit TSC noise
The fault engine SHALL support a `ClockJitter` fault variant that adds a
random offset (drawn from the VM's deterministic RNG) in the range
`[-bound_tsc, +bound_tsc]` to the virtual TSC value written by
`sync_tsc_to_guest()` on each exit. This models an unstable oscillator
or noisy clock source. The jitter persists until cleared by a
`ClockJitter { bound_tsc: 0 }` fault.

#### Scenario: Per-exit jitter applied
- **WHEN** `ClockJitter { target: 0, bound_tsc: 500 }` is active
- **THEN** on each VM exit for VM 0, the TSC written to the guest SHALL differ from the ideal virtual TSC by at most ±500 ticks
- **AND** the jitter amount SHALL be deterministic (seeded RNG)

#### Scenario: Jitter does not accumulate
- **WHEN** jitter is active across multiple exits
- **THEN** each jitter offset SHALL be computed fresh from the ideal virtual TSC, not from the previously jittered value
- **AND** the underlying virtual TSC SHALL advance normally (jitter is cosmetic)

#### Scenario: Clear jitter
- **WHEN** `ClockJitter { target: 0, bound_tsc: 0 }` fires
- **THEN** VM 0's TSC jitter SHALL be removed and `sync_tsc_to_guest()` SHALL write the exact virtual TSC value

#### Scenario: Snapshot preserves jitter config
- **WHEN** a snapshot is taken while jitter is active
- **AND** the snapshot is restored
- **THEN** the jitter bound SHALL still be in effect after restore

### Requirement: Random generation includes clock faults
The FaultEngine and ScheduleMutator random generators SHALL include
`ClockFreeze` and `ClockJitter` in their selection pool.

#### Scenario: Random pool coverage
- **WHEN** generating 1000 random faults with num_vms >= 1
- **THEN** at least one `ClockFreeze` and one `ClockJitter` SHALL appear

### Requirement: Serialization backward compatibility
New clock fault variants SHALL serialize/deserialize without breaking
existing checkpoint or bug report JSON files.

#### Scenario: Roundtrip serialization
- **WHEN** a fault schedule containing `ClockFreeze` and `ClockJitter` is serialized to JSON
- **AND** deserialized back
- **THEN** the schedule SHALL be identical to the original
