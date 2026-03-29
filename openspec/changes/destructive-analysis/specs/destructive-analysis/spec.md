## ADDED Requirements

### Requirement: Memory reading from snapshots
The debugger SHALL implement `read_memory()` to extract guest memory content from the snapshot at the current debugger position. It MUST support both guest physical addresses (GPA) and guest virtual addresses (GVA), with GVA translated to GPA via CR3 page table walks.

#### Scenario: Read physical memory at current position
- **WHEN** the debugger calls `read_memory(vm_index, gpa, size)` with a guest physical address
- **THEN** it returns the exact bytes from the guest memory at that GPA in the current checkpoint snapshot

#### Scenario: Read virtual memory with page table translation
- **WHEN** the debugger calls `read_memory(vm_index, gva, size)` with a guest virtual address  
- **THEN** it translates GVA to GPA using CR3 from the vCPU snapshot, then returns memory content at the resulting GPA

#### Scenario: Handle invalid addresses gracefully
- **WHEN** the debugger tries to read from unmapped GVA or out-of-bounds GPA
- **THEN** it returns a `ReplayError::InvalidAddress` instead of crashing or returning garbage data

### Requirement: Register reading from snapshots  
The debugger SHALL implement `read_registers()` to extract complete vCPU register state from `VcpuSnapshot` at the current position. It MUST populate all fields in `RegisterState` from the corresponding `kvm_regs`, `kvm_sregs`, and `kvm_fpu` structures.

#### Scenario: Read general purpose registers
- **WHEN** the debugger calls `read_registers(vm_index, vcpu_index)`
- **THEN** it returns `RegisterState` with `rax`, `rbx`, `rcx`, etc. populated from `vcpu_snapshot.regs`

#### Scenario: Read segment and control registers
- **WHEN** reading registers from a vCPU snapshot
- **THEN** `RegisterState` includes `cs`, `ss`, `rip`, `rsp`, `rflags` from the appropriate snapshot fields

#### Scenario: Handle invalid vCPU index
- **WHEN** the debugger requests registers for a non-existent vCPU index
- **THEN** it returns `ReplayError::InvalidVcpu` with the invalid index

### Requirement: Memory modification with replay
The debugger SHALL implement `write_memory()` that modifies guest memory at the current position and replays forward to show the effect. It MUST build a `MemoryModification` and use the existing `counterfactual()` replay mechanism.

#### Scenario: Poke memory and replay forward
- **WHEN** the debugger calls `write_memory(vm_index, addr, data)` and then `step_forward(N)`  
- **THEN** memory at `addr` contains the new `data` during subsequent execution for N ticks

#### Scenario: Memory modifications are ephemeral
- **WHEN** a debug session modifies memory and then navigates to a different tick
- **THEN** the memory changes are lost and the original recorded values are restored

#### Scenario: Multiple memory modifications in one operation
- **WHEN** the debugger applies multiple `MemoryModification` via `counterfactual(modifications, ticks)`
- **THEN** all modifications take effect simultaneously before replay begins

### Requirement: Register modification with replay
The debugger SHALL implement `set_register()` that modifies vCPU register values at the current position and replays forward. It MUST introduce a new `RegisterModification` type parallel to `MemoryModification`.

#### Scenario: Set general purpose register and replay
- **WHEN** the debugger calls `set_register(vm_index, vcpu_index, "rax", 0x1337)` and replays forward
- **THEN** the vCPU's `rax` register contains `0x1337` during subsequent execution

#### Scenario: Register modifications combine with memory modifications  
- **WHEN** the debugger applies both `MemoryModification` and `RegisterModification` in one `counterfactual()` call
- **THEN** both memory and register changes take effect before replay begins

#### Scenario: Invalid register names are rejected
- **WHEN** the debugger tries to set a non-existent register name like "xyz"
- **THEN** it returns `ReplayError::InvalidRegister` without modifying any state

### Requirement: CLI integration for interactive debugging
The debugger CLI SHALL support interactive commands for memory and register access: `read`, `regs`, `poke`, `setreg`. Each command MUST provide human-readable output with hex formatting for addresses and values.

#### Scenario: Interactive memory reading
- **WHEN** the user types `read 0x401000 64` in the debugger CLI
- **THEN** it displays 64 bytes from address 0x401000 in hex dump format with ASCII representation

#### Scenario: Register display
- **WHEN** the user types `regs 0` to show vCPU 0 registers  
- **THEN** it displays all general purpose, segment, and control registers in name=value format

#### Scenario: Memory poking  
- **WHEN** the user types `poke 0x401000 deadbeef` 
- **THEN** it writes the bytes `0xdeadbeef` to address `0x401000` and confirms the modification

#### Scenario: Register setting
- **WHEN** the user types `setreg 0 rax 0x1337`
- **THEN** it sets vCPU 0's `rax` register to `0x1337` and shows the updated value