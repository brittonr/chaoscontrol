## 1. Memory Reading

- [ ] 1.1 Implement `read_memory(vm_index, gpa, size)` in Debugger — access GuestMemoryMmap from snapshot at current checkpoint
- [ ] 1.2 Add `translate_gva(vm_index, vcpu_index, gva)` helper that walks x86_64 4-level page tables using CR3 from VcpuSnapshot.sregs
- [ ] 1.3 Add `read_memory_virtual(vm_index, vcpu_index, gva, size)` that translates then reads

## 2. Register Reading

- [ ] 2.1 Implement `read_registers(vm_index, vcpu_index)` — populate RegisterState from VcpuSnapshot (kvm_regs + kvm_sregs)
- [ ] 2.2 Add vcpu_index bounds checking (return ReplayError for invalid indices)

## 3. Register Modification

- [ ] 3.1 Define `RegisterModification { vm_index, vcpu_index, register: RegisterName, value: u64 }` and `RegisterName` enum
- [ ] 3.2 Implement `set_register()` on Debugger — applies RegisterModification to VcpuSnapshot, then replays forward via counterfactual
- [ ] 3.3 Extend `ReplayEngine::replay_with_modification` to accept register modifications alongside memory modifications

## 4. Memory Writing

- [ ] 4.1 Implement `poke_memory(vm_index, gpa, bytes)` — builds MemoryModification and calls counterfactual with configurable ticks
- [ ] 4.2 Add GVA variant `poke_memory_virtual(vm_index, vcpu_index, gva, bytes)` using translate_gva

## 5. CLI Commands

- [ ] 5.1 Add `read <gpa> <size>` command to `chaoscontrol-replay debug` — hex dump output
- [ ] 5.2 Add `regs <vm> [vcpu]` command — print all registers in name=hex format
- [ ] 5.3 Add `poke <gpa> <hex_bytes>` command — write memory and replay forward, show diff
- [ ] 5.4 Add `setreg <vm> <vcpu> <reg> <hex_value>` command — modify register and replay forward
- [ ] 5.5 Support 0x prefix for hex address/value parsing

## 6. Testing

- [ ] 6.1 Unit test: read_memory returns correct bytes from known snapshot GPA
- [ ] 6.2 Unit test: read_registers populates all RegisterState fields from VcpuSnapshot
- [ ] 6.3 Unit test: poke_memory changes guest state and replay produces different output
- [ ] 6.4 Unit test: set_register with invalid register name returns error
- [ ] 6.5 Unit test: translate_gva returns error for unmapped address
- [ ] 6.6 Run `cargo clippy --workspace` clean
