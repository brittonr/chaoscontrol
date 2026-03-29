## Context

The time-travel debugger in `chaoscontrol-replay/src/debugger.rs` can navigate to any tick via `goto()`, `rewind()`, `step_forward()`. It builds `DebugState` with events and serial output, but state introspection is limited. The `read_memory()` and `read_registers()` methods exist but return "not yet implemented" errors. The `counterfactual()` method can apply `MemoryModification` and replay forward, but there's no way to modify registers.

Snapshots are the key: `VmSnapshot` contains `Vec<VcpuSnapshot>` (registers) and guest memory state. The debugger tracks `current_checkpoint` and can access the snapshot at that position. Memory is accessible via `GuestMemoryMmap`, registers via `kvm_regs`/`kvm_sregs`/`kvm_fpu` fields in `VcpuSnapshot`.

## Goals / Non-Goals

**Goals:**
- Read guest memory at any address (GPA or GVA) from the current debugger position
- Read all registers (general purpose, segment, control, FPU) from any vCPU 
- Write memory and register values, then replay forward to observe effects
- Support both destructive analysis (permanent until restore) and ephemeral "what if" tests
- CLI integration with simple commands: `read 0x401000 64`, `regs 0`, `poke 0x401000 0xdeadbeef`, `setreg 0 rax 0x1337`

**Non-Goals:**
- Modifying the recording file (all changes are session-only)
- Supporting live debugging (this is replay-only, not live attach)
- Advanced address translation (flat GVA→GPA via CR3 is sufficient)
- Symbolic debugging or source-level integration

## Decisions

**1. Memory access via snapshots, not live VM state**

Read operations pull from the snapshot at `current_checkpoint`, not from any running VM. This is consistent with the replay architecture — the debugger shows state as it was recorded, not as it is now. For positions between checkpoints, use the most recent checkpoint (slight staleness is acceptable).

Alternative: Replay to exact tick before reading. Rejected — too expensive for read-only operations.

**2. GVA→GPA translation via page table walk**

Virtual addresses require walking the guest page tables starting from CR3. The VMM already has `GuestMemoryMmap` for physical access. Add a `translate_gva_to_gpa()` helper that reads CR3, walks page table levels, returns GPA. For GPA input, bypass translation.

Alternative: Only support GPA. Rejected — debugging often works with virtual addresses (code at 0x401000, stack at 0x7ffe...).

**3. RegisterModification parallel to MemoryModification**

Add a new `RegisterModification` struct with `vm_index`, `vcpu_index`, `register_name`, `value`. The `counterfactual()` method should accept both memory and register modifications in a single call. The replay engine applies register changes by modifying the `VcpuSnapshot` before calling `snapshot.restore()`.

Alternative: Separate `set_register_counterfactual()` method. Rejected — atomic memory+register changes are useful.

**4. CLI commands in debug mode, not new binary**

Extend the existing `chaoscontrol-replay debug <recording>` subcommand with interactive commands. The debugger is already interactive — this adds more commands to the existing REPL. Commands: `read <addr> <size>`, `regs <vm>`, `poke <addr> <hex>`, `setreg <vm> <reg> <value>`.

## Risks / Trade-offs

**[Page table walk complexity]** → x86_64 page tables have 4 levels (PML4, PDP, PD, PT) with various flags and formats. A bug in translation logic could return wrong addresses or crash. Mitigation: Start with simple flat translation, add validation, test against known GVA/GPA pairs from guest debug info.

**[Snapshot staleness between checkpoints]** → If the debugger is at tick 1500 but the last checkpoint is at tick 1000, memory/register reads show state from tick 1000, not tick 1500. This could be confusing during debugging. Mitigation: Document this clearly, consider replaying to exact tick for critical reads.

**[Register modification complexity]** → x86_64 has ~20 general-purpose registers plus segment registers, control registers, MSRs. The `RegisterState` struct has them, but mapping string names like "rax" to fields requires parsing. Also, some register changes (CR3, segment changes) affect address translation. Mitigation: Start with GP registers only, validate changes don't break the VM.

**[Replay cost for writes]** → Every `poke` or `setreg` triggers replay from the current checkpoint to observe effects. For checkpoints 1000 ticks apart, this could take ~100ms. Repeated modifications compound the cost. Mitigation: Batch modifications, cache replay results for repeated "what if" tests.