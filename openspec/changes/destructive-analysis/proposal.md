## Why

The time-travel debugger can navigate to any tick and inspect events, but `read_memory()` and `read_registers()` are stubs returning "not yet implemented". There's no way to write registers or poke arbitrary memory. Bug investigation requires understanding precise state at failure points — what values were in memory? Which registers held corrupted data? What happens if we change a single bit and continue execution? This capability exists in production debuggers (gdb, lldb) and hypervisor-assisted tools (Antithesis). ChaosControl's deterministic replay makes destructive "what if" analysis particularly powerful — changes don't corrupt the recording.

## What Changes

- Implement `read_memory()` and `read_registers()` in the debugger by accessing snapshots at the current position
- Add `write_memory()` and `set_register()` methods that modify state and replay forward to show effects  
- Support both guest physical addresses (GPA) and guest virtual addresses (GVA) via page table walks
- Add `RegisterModification` type parallel to existing `MemoryModification`
- Extend CLI with interactive debug commands: `read`, `regs`, `poke`, `setreg`
- All modifications are ephemeral — they apply only to the current debug session, not the recorded trace

## Capabilities

### New Capabilities
- `destructive-analysis`: Read/write guest memory and registers at any debugger position for root cause analysis

### Modified Capabilities

(none — extending existing debugger without changing recorded behavior)

## Impact

- **Code changed**: `crates/chaoscontrol-replay/src/debugger.rs` (implement read/write methods), `crates/chaoscontrol-replay/src/replay.rs` (RegisterModification type), `crates/chaoscontrol-vmm/src/snapshot.rs` (expose memory access from snapshots), CLI integration in replay binary
- **No API changes**: New methods on existing Debugger struct, compatible addition
- **Memory access patterns**: GVA→GPA translation via CR3 page tables, direct GPA access to GuestMemoryMmap
- **Performance**: Read operations are O(1) snapshot lookups. Write operations trigger replay from current checkpoint — cost scales with ticks-to-replay, not modification count
- **Debugging workflow**: Find bug with explorer → Load recording in debugger → Navigate to failure → Read state → Poke values → Replay forward to see counterfactual effects