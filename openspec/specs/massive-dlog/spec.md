# Massive Dlog Specification

## Purpose

Defines the canonical ChaosControl requirements for massive dlog.

## Requirements
### Requirement: Register state in dlog records
The dlog writer SHALL store the guest RIP in every DlogRecord (already present)
and SHALL add RSP, RAX, and RFLAGS to the `extra` field for IoIn, IoOut,
MmioRead, MmioWrite, Hlt, and Hypercall tags.

#### Scenario: Register fields populated on port I/O
- **WHEN** a VM exit of type IoIn or IoOut occurs
- **THEN** the emitted DlogRecord SHALL contain the guest RIP in the `rip` field and RSP in the first 4 bytes of `extra`

#### Scenario: Non-register tags leave extra unchanged
- **WHEN** a Marker or CoverageSync record is emitted
- **THEN** the `extra` field SHALL retain its tag-specific meaning (no register overlay)

### Requirement: Full register dump records
The dlog writer SHALL support a DlogTag::RegisterDump tag that captures a
full 64-byte snapshot of selected general-purpose registers (RIP, RSP, RAX,
RBX, RCX, RDX, RSI, RDI, RFLAGS) written as a standalone record.

#### Scenario: Periodic register dump
- **WHEN** `dlog_register_interval` is set to N in VmConfig
- **THEN** a RegisterDump record SHALL be emitted every N VM exits

#### Scenario: Register dump disabled by default
- **WHEN** `dlog_register_interval` is 0 or unset
- **THEN** no RegisterDump records SHALL be emitted

### Requirement: Memory page hashing
The dlog writer SHALL support a DlogTag::MemoryHash tag whose `data` field
contains a CRC32 hash of a 4 KB guest memory page and whose
`port_or_addr_lo`/`port_or_addr_hi` fields encode the page-frame number.

#### Scenario: Hash at snapshot boundary
- **WHEN** a snapshot is taken and `dlog_memory_hash` is enabled
- **THEN** one MemoryHash record SHALL be emitted per tracked page

#### Scenario: Hash divergence detectable by dlog_diff
- **WHEN** two dlog files are compared and a MemoryHash record differs
- **THEN** `dlog_diff` SHALL report the page-frame number and both CRC32 values in the DiffResult

### Requirement: Cross-VM tick markers
The SimulationController SHALL emit a DlogTag::TickMarker record into every
per-VM dlog at the start of each simulation tick, carrying the global tick
counter in the `data` field.

#### Scenario: Tick markers present in multi-VM dlog
- **WHEN** a 3-VM simulation runs for 100 ticks with dlog enabled
- **THEN** each VM's dlog file SHALL contain 100 TickMarker records with matching tick values

#### Scenario: Cross-VM diff alignment
- **WHEN** `dlog_diff` compares two VM dlog files from the same run
- **THEN** TickMarker records with the same `data` value SHALL appear at corresponding positions

### Requirement: Dlog CLI subcommand
The `chaoscontrol-replay` binary SHALL expose a `dlog` subcommand with
sub-subcommands `dump`, `diff`, and `stats`.

#### Scenario: dlog dump
- **WHEN** `chaoscontrol-replay dlog dump --file vm_0.dlog --from 100 --count 20`
- **THEN** 20 human-readable dlog records starting at index 100 SHALL be printed to stdout

#### Scenario: dlog diff
- **WHEN** `chaoscontrol-replay dlog diff --file-a run1/vm_0.dlog --file-b run2/vm_0.dlog`
- **THEN** the first divergence (or "Identical") SHALL be printed with a 5-record context window

#### Scenario: dlog diff strict mode
- **WHEN** `--strict` flag is passed to `dlog diff`
- **THEN** RIP SHALL be included in the comparison

#### Scenario: dlog stats
- **WHEN** `chaoscontrol-replay dlog stats --file vm_0.dlog`
- **THEN** a per-tag record count and total record count SHALL be printed
