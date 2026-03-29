## Context

The dlog (determinism log) infrastructure records a 64-byte `DlogRecord` for
every VM exit. Two logs from same-seed runs can be diffed to find the exact
exit where execution diverged. The current record stores tag, virtual TSC,
exit count, RIP, port/MMIO address, and 8+8 bytes of data/extra. This is
enough to locate *which* exit diverged, but not *why* — you still need a
debugger session to inspect register and memory state.

The replay debugger (`Debugger<R>`) supports time-travel navigation (goto,
rewind, step_forward, goto_bug) and has stubs for `read_memory`,
`read_registers`, and `counterfactual`, all returning "not yet implemented".
The `SimulationRunner` trait doesn't expose memory/register access, so the
debugger has no path to the live VM.

The `DeterministicVm` already has `memory() -> &GuestMemoryManager` (which
wraps a `GuestMemoryMmap` with `read_slice`/`write_slice`), and
`VcpuSnapshot::capture` reads all registers. The plumbing from VM → runner →
debugger is the gap.

## Goals / Non-Goals

**Goals:**

- Register context (RSP + RFLAGS) in every dlog record for richer diff output
- Optional full RegisterDump records at configurable intervals
- Memory page CRC32 hashing at snapshot boundaries to narrow divergence to
  specific pages
- Cross-VM tick markers so multi-VM dlog files can be time-aligned
- CLI for dump/diff/stats without writing Rust
- Working `read_memory`, `write_memory`, `read_registers`, `set_registers`
  on the debugger, wired through SimulationRunner to live VMs
- `counterfactual()` that applies both memory and register modifications

**Non-Goals:**

- Symbolic execution or automatic root-cause analysis
- Continuous memory hashing (every exit) — too expensive; snapshot-boundary only
- GDB stub or remote debug protocol — this is programmatic API + CLI
- Persistent forked timelines — counterfactual is one-shot, not a branch tree

## Decisions

### D1: RSP + RFLAGS in the existing `extra` field

Pack RSP (4 bytes, low 32 bits — sufficient for stack-relative comparison)
and RFLAGS (4 bytes, low 32 bits — includes IF, DF, CF, ZF which are the
bits that matter for divergence) into the 8-byte `extra` field on
exit-type records (IoIn, IoOut, MmioRead, MmioWrite, Hlt, Hypercall).

**Alternative**: Add a second 64-byte record per exit with full registers.
Rejected — doubles I/O bandwidth and the 4-byte truncation captures the
useful bits for divergence diagnosis. Full dumps are available via the
separate RegisterDump tag for deep investigation.

### D2: CRC32 for memory hashing (not SHA-256)

CRC32 is ~1 cycle/byte on x86 (SSE4.2 `crc32` instruction) and catches any
single-page divergence with effectively zero false positives for our use case
(comparing two runs of the same software with the same seed). SHA-256 would be
~30× slower for no practical benefit.

Use the `crc32fast` crate (already widely used, SSE4.2 + ARM CRC intrinsics).

### D3: Hash only "interesting" pages, not all of memory

Hashing 256 MB at every snapshot would take ~50ms — acceptable but wasteful.
Instead, hash pages that the VM has written to since the last hash point. The
coverage bitmap page (0xE0000) and the hypercall page (0xFE000) are always
included. The dirty page set comes from scanning the CoW block device's dirty
map plus a fixed set of ranges (stack, kernel BSS, heap).

For the initial implementation: hash a fixed set of well-known pages (coverage,
hypercall, stack area, first 1 MB) plus any pages in the block device's dirty
set. This keeps complexity low. A future iteration could use KVM's dirty page
tracking (`KVM_GET_DIRTY_LOG`).

### D4: TickMarker records emitted by the controller, not the VM

The controller owns the global tick counter and orchestrates per-VM execution.
It calls `vm.dlog_tick_marker(tick)` on each VM before running that VM's
quantum. This keeps the tick → dlog correlation tight without the VM needing
to know about simulation ticks.

### D5: SimulationRunner trait extension for memory/register access

Add four methods to `SimulationRunner`:

```rust
fn read_memory(&self, vm: usize, addr: u64, size: usize) -> Result<Vec<u8>, ReplayError>;
fn write_memory(&mut self, vm: usize, addr: u64, data: &[u8]) -> Result<(), ReplayError>;
fn read_registers(&self, vm: usize, vcpu: usize) -> Result<RegisterState, ReplayError>;
fn set_registers(&mut self, vm: usize, vcpu: usize, regs: &RegisterState) -> Result<(), ReplayError>;
```

`RealSimulationRunner` delegates to `controller.vm(index).memory().inner()`
and `controller.vm(index).vcpus[vcpu].get_regs()` / `set_regs()`.

The `MockRunner` in tests returns fixed data or records calls.

### D6: RegisterModification struct parallel to MemoryModification

```rust
pub struct RegisterModification {
    pub vm_index: usize,
    pub vcpu: usize,
    pub changes: BTreeMap<Register, u64>,
}

pub enum Register {
    Rip, Rsp, Rax, Rbx, Rcx, Rdx, Rsi, Rdi, Rbp,
    R8, R9, R10, R11, R12, R13, R14, R15,
    Rflags,
}
```

`counterfactual()` accepts `Vec<MemoryModification>` +
`Vec<RegisterModification>`. Memory patches apply first (via
`write_memory`), then register overrides (via `set_registers`). Order
matters — a register override to RSP after a stack memory patch is the
common pattern.

### D7: DeterministicVm public API for memory/register access

Expose on `DeterministicVm`:

```rust
pub fn read_guest_memory(&self, addr: u64, size: usize) -> Result<Vec<u8>, VmError>;
pub fn write_guest_memory(&self, addr: u64, data: &[u8]) -> Result<(), VmError>;
pub fn read_vcpu_registers(&self, vcpu: usize) -> Result<RegisterState, VmError>;
pub fn set_vcpu_registers(&mut self, vcpu: usize, regs: &RegisterState) -> Result<(), VmError>;
```

These wrap the existing `memory().inner().read_slice()` /
`write_slice()` and `vcpu.get_regs()` / `set_regs()` / `get_sregs()`.

## Risks / Trade-offs

- **[Dlog file size]** RegisterDump records at interval=100 on a 100K-exit
  run add ~64KB (1000 records × 64 bytes). Negligible.
  Memory hashing adds ~1 record per tracked page per snapshot. With 256
  tracked pages and snapshots every 1000 ticks, that's 256 × 64 = 16KB per
  snapshot. Acceptable.
  → Mitigation: both features are opt-in (disabled by default).

- **[Register access requires stopped vCPU]** `get_regs()` / `set_regs()`
  are only valid when the vCPU is not in `KVM_RUN`. Since the debugger
  operates on a restored snapshot (vCPU not running), this is safe. The
  danger is calling these from the exploration loop while a vCPU is live.
  → Mitigation: document that these methods require the VM to not be in
  `run_bounded()`.

- **[Memory writes can corrupt VM state]** `write_guest_memory` can write
  anywhere — page tables, GDT, kernel text. This is intentional for
  destructive analysis but dangerous.
  → Mitigation: these methods are clearly named as destructive; the
  debugger is the only intended caller.

- **[CRC32 false positives]** Two different page contents could have the
  same CRC32 (probability ~2⁻³²). For our use case (same software, same
  seed, divergence is typically a single bit or counter), this is not a
  practical concern.
