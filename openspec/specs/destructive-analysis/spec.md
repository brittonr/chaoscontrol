## ADDED Requirements

### Requirement: Read guest memory
The Debugger SHALL support reading arbitrary guest physical memory at the
current replay position.

#### Scenario: Read a single page
- **WHEN** `read_memory(vm_index=0, address=0x1000, size=4096)` is called after `goto(tick)`
- **THEN** 4096 bytes of guest physical memory starting at GPA 0x1000 SHALL be returned

#### Scenario: Read out-of-bounds address
- **WHEN** `read_memory` is called with an address beyond guest memory size
- **THEN** an error SHALL be returned (not a panic)

### Requirement: Read guest registers
The Debugger SHALL support reading vCPU register state at the current replay
position.

#### Scenario: Read BSP registers
- **WHEN** `read_registers(vm_index=0, vcpu=0)` is called after `goto(tick)`
- **THEN** a RegisterState containing RIP, RSP, RAX through R15, RFLAGS, segment registers, and CR0/CR3/CR4 SHALL be returned

#### Scenario: Read AP registers in SMP
- **WHEN** `read_registers(vm_index=0, vcpu=1)` is called on a 2-vCPU VM
- **THEN** the AP's register state SHALL be returned

### Requirement: Write guest memory (poke_memory)
The Debugger SHALL support writing arbitrary bytes to guest physical memory
at the current replay position, creating a "what if" fork.

#### Scenario: Poke a single byte
- **WHEN** `poke_memory(vm_index=0, address=0x2000, data=[0x42])` is called
- **THEN** guest physical address 0x2000 SHALL contain 0x42
- **AND** subsequent `step_forward` SHALL execute with the modified memory

#### Scenario: Poke and continue
- **WHEN** `poke_memory` is called followed by `step_forward(100)`
- **THEN** the simulation SHALL run for 100 ticks using the modified guest memory

#### Scenario: Poke does not mutate the underlying recording
- **WHEN** `poke_memory` is called and then `goto(0)` rewinds to the start
- **THEN** the original memory contents SHALL be intact

### Requirement: Set guest registers
The Debugger SHALL support writing vCPU register values at the current replay
position.

#### Scenario: Set RIP
- **WHEN** `set_register(vm_index=0, vcpu=0, reg=RIP, value=0x5000)` is called
- **THEN** the vCPU's instruction pointer SHALL be 0x5000
- **AND** subsequent `step_forward` SHALL resume execution from 0x5000

#### Scenario: Set general-purpose register
- **WHEN** `set_register(vm_index=0, vcpu=0, reg=RAX, value=0xFF)` is called
- **THEN** RAX SHALL contain 0xFF on the next VM entry

### Requirement: Counterfactual replay with register overrides
The `counterfactual()` method SHALL accept both memory modifications and
register modifications.

#### Scenario: Counterfactual with register override
- **WHEN** `counterfactual(modifications=[], register_mods=[{vm:0, vcpu:0, reg:RAX, value:1}], ticks=200)` is called
- **THEN** the replay SHALL execute 200 ticks with RAX=1 from the current checkpoint

#### Scenario: Combined memory and register override
- **WHEN** `counterfactual` is called with both a memory patch and a register override
- **THEN** both modifications SHALL be applied before resuming execution

### Requirement: SimulationRunner memory/register access
The `SimulationRunner` trait SHALL expose methods for reading and writing
guest memory and registers so the Debugger can access live VM state.

#### Scenario: Read memory through runner
- **WHEN** `runner.read_memory(vm_index, address, size)` is called
- **THEN** bytes from the guest's physical memory SHALL be returned

#### Scenario: Write memory through runner
- **WHEN** `runner.write_memory(vm_index, address, data)` is called
- **THEN** the bytes SHALL be written to the guest's physical memory

#### Scenario: Read registers through runner
- **WHEN** `runner.read_registers(vm_index, vcpu)` is called
- **THEN** the vCPU's current register state SHALL be returned

#### Scenario: Set registers through runner
- **WHEN** `runner.set_registers(vm_index, vcpu, register_state)` is called
- **THEN** the vCPU's registers SHALL be updated
