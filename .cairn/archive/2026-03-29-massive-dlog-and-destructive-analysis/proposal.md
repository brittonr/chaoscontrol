## Why

The dlog infrastructure records per-exit events but lacks register snapshots,
memory hashing, and cross-VM correlation — the "paranoid mode" that Antithesis
uses to diagnose subtle non-determinism regressions in minutes instead of
hours. Separately, the replay debugger stubs out `read_memory`,
`read_registers`, `poke_memory`, and `set_register` — the destructive
analysis primitives needed for "what if" counterfactual debugging from a
checkpoint.

## What Changes

- **Dlog register snapshots**: Record RIP + RSP + RAX + RFLAGS in every
  dlog record's `extra` field, plus optional full-register records
  (DlogTag::RegisterDump) at configurable intervals.
- **Memory page hashing**: New DlogTag::MemoryHash records that store a
  CRC32 of selected 4 KB pages at checkpoint boundaries, enabling fast
  "which page diverged" binary search between two runs.
- **Cross-VM dlog correlation**: Controller emits DlogTag::Tick markers
  with the global tick counter into each per-VM dlog, so multi-VM diff
  can align on simulation time.
- **Dlog CLI subcommand**: `chaoscontrol-replay dlog dump|diff|stats`
  for inspecting and comparing dlog files without writing custom code.
- **Destructive analysis in the debugger**: Wire `read_memory`,
  `read_registers`, `poke_memory` (write guest physical memory),
  `set_register` (modify vCPU registers) through SimulationRunner
  and into the live VM, enabling counterfactual replay from any
  checkpoint with modified state.
- **Counterfactual with register modifications**: Extend `counterfactual()`
  to accept register overrides alongside memory patches.

## Capabilities

### New Capabilities
- `massive-dlog`: Extended determinism logging with register snapshots,
  memory hashing, cross-VM tick markers, and CLI tooling.
- `destructive-analysis`: Read/write guest memory and registers from the
  replay debugger for "what if" counterfactual analysis.

### Modified Capabilities

(none — no existing spec-level requirements change)

## Impact

- **crates/chaoscontrol-vmm**: dlog.rs expanded with new tags and record
  helpers; vm.rs emits richer records; controller.rs emits tick markers.
- **crates/chaoscontrol-replay**: debugger.rs implements read/write
  memory/registers; replay.rs wires MemoryModification and new
  RegisterModification through SimulationRunner; CLI gains dlog
  subcommand.
- **crates/chaoscontrol-vmm (public API)**: DeterministicVm gets
  `read_guest_memory`, `write_guest_memory`, `read_registers`,
  `set_registers` public methods. SimulationRunner trait gains matching
  methods.
