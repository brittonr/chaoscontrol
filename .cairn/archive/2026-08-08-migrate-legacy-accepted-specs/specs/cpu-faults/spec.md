# Cpu Faults Specification

## Purpose

Defines the canonical ChaosControl requirements for cpu faults.

## Requirements
### Requirement: CpuBitflip fault corrupts a general-purpose register
The fault engine SHALL support a `CpuBitflip` fault variant that flips a
single bit in one of the target VM's general-purpose registers (RAX–R15)
at the next tick boundary. The register and bit position are specified in
the fault. This models single-event upsets (cosmic ray bitflips, ECC failures).

#### Scenario: Bitflip applied to register
- **WHEN** `CpuBitflip { target: 0, vcpu: 0, register: Rax, bit: 12 }` fires
- **THEN** bit 12 of vCPU 0's RAX on VM 0 SHALL be toggled (0→1 or 1→0)
- **AND** all other registers SHALL remain unchanged

#### Scenario: Bitflip is one-shot
- **WHEN** a `CpuBitflip` fault fires
- **THEN** the bit SHALL be flipped exactly once (not persisted as a recurring fault)

#### Scenario: Register enum covers all GPRs
- **WHEN** constructing a `CpuBitflip` fault
- **THEN** the `register` field SHALL accept any of: Rax, Rbx, Rcx, Rdx, Rsi, Rdi, Rbp, Rsp, R8–R15

#### Scenario: Bit position bounds
- **WHEN** `bit` is >= 64
- **THEN** the fault SHALL be silently ignored (no-op) rather than panicking

#### Scenario: Implementation uses KVM get/set regs
- **WHEN** applying the bitflip
- **THEN** the controller SHALL read the vCPU registers via `get_regs()`, XOR the target bit, and write back via `set_regs()`

### Requirement: CpuStall fault pauses a single vCPU
The fault engine SHALL support a `CpuStall` fault variant that prevents a
specific vCPU from being scheduled for `duration_ticks` ticks. Other vCPUs
on the same VM continue running. This models a core entering deep C-state,
thermal throttling, or a microcode assist stall.

#### Scenario: vCPU stalled
- **WHEN** `CpuStall { target: 0, vcpu: 1, duration_ticks: 50 }` fires
- **THEN** vCPU 1 on VM 0 SHALL be skipped by the scheduler for 50 ticks
- **AND** vCPU 0 SHALL continue to be scheduled normally

#### Scenario: Stall on single-vCPU VM
- **WHEN** `CpuStall { target: 0, vcpu: 0, duration_ticks: 10 }` fires on a VM with 1 vCPU
- **THEN** the VM SHALL effectively pause for 10 ticks (same as ProcessPause)

#### Scenario: Stall expires
- **WHEN** the stall duration elapses
- **THEN** the vCPU SHALL automatically rejoin the scheduling rotation
- **AND** no explicit "unstall" fault is required

#### Scenario: Snapshot preserves stall state
- **WHEN** a snapshot is taken during an active `CpuStall`
- **AND** the snapshot is restored
- **THEN** the remaining stall duration SHALL be preserved

### Requirement: New FaultCategory for CPU faults
A `FaultCategory::Cpu` variant SHALL be added to classify `CpuBitflip` and
`CpuStall`.

#### Scenario: Category classification
- **WHEN** calling `.category()` on a `CpuBitflip` or `CpuStall` fault
- **THEN** the result SHALL be `FaultCategory::Cpu`

### Requirement: Random generation includes CPU faults
The FaultEngine and ScheduleMutator random generators SHALL include
`CpuBitflip` and `CpuStall` in their selection pool.

#### Scenario: Random pool coverage
- **WHEN** generating 1000 random faults with num_vms >= 1
- **THEN** at least one `CpuBitflip` and one `CpuStall` SHALL appear
