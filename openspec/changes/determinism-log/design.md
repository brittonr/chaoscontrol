## Context

ChaosControl's determinism rests on ~15 mechanisms (CPUID filtering,
virtual TSC, suppressed PIT, etc.). When any of them break, the symptom
is identical: two runs of the same seed produce different results. The
cause could be anywhere in the exit loop. Today's tools for debugging
this:

1. **eBPF tracing** (`chaoscontrol-trace`): external, needs sudo,
   captures KVM tracepoints but not VMM-internal state (scheduler
   decisions, SDK hypercall payloads, fault engine state).
2. **env_logger**: ad-hoc `info!()` calls, not structured, can't be
   compared programmatically.
3. **RecordedEvent**: high-level (faults, assertions, serial). One event
   per fault/assertion, not per exit.

None of these capture the per-exit state needed to pinpoint divergence.

## Goals / Non-Goals

**Goals:**
- Zero measurable overhead when disabled (Option check on hot path).
- Capture every VM exit with enough context to locate divergence: exit
  type, port/address, data, virtual TSC, exit count, vCPU index, RIP.
- Capture scheduler switches, SDK hypercalls, interrupt injections,
  fault applications, and snapshot/restore boundaries.
- Binary format for throughput — handle 500K exits/sec without
  becoming the bottleneck.
- Two-log diff tool: given runs A and B (same seed), print the first
  record where they diverge and the 5 records before it for context.
- Text dump for manual inspection.

**Non-Goals:**
- Replacing eBPF tracing (that captures host-kernel-level detail we
  can't see from userspace).
- Replacing RecordedEvent (that serves the replay/triage workflow at a
  higher abstraction level).
- Memory-mapped ring buffer or real-time streaming. Disk file is fine.
- Compression. Runs are short enough that raw binary is manageable
  (~6 MB per 100K exits at 64 bytes/record).

## Decisions

### Record format: fixed 64-byte binary struct

```rust
#[repr(C, packed)]
struct DlogRecord {
    /// Sequence number — monotonically increasing per VM.
    seq: u64,
    /// Virtual TSC at time of record.
    virtual_tsc: u64,
    /// VM exit count (matches vm.exit_count).
    exit_count: u64,
    /// Guest RIP at the exit point.
    rip: u64,
    /// Record type tag.
    tag: u8,
    /// Active vCPU index.
    vcpu: u8,
    /// I/O port or MMIO address (low 16 bits, or full 32 for MMIO).
    port_or_addr: u32,
    /// First 8 bytes of I/O data, SDK command, or fault info.
    data: [u8; 8],
    /// Extra context (varies by tag): scheduler quantum remaining,
    /// SDK response, interrupt vector, etc.
    extra: [u8; 8],
    /// Padding to 64 bytes.
    _pad: [u8; 2],
}
```

Fixed size means no framing, no length prefixes, trivial seeking
(`record_n = &file[n * 64 .. (n+1) * 64]`), and the diff tool can
operate record-by-record without parsing.

**Why not variable-length?** MMIO data can be up to 8 bytes (already
fits). SDK hypercall payloads are 4096 bytes but we only need the
command ID + first argument for diffing — the full page is in guest
memory. Fixed records keep the writer branchless.

### Tag enum

```rust
#[repr(u8)]
enum DlogTag {
    IoIn = 1,
    IoOut = 2,
    MmioRead = 3,
    MmioWrite = 4,
    Hlt = 5,
    Shutdown = 6,
    Hypercall = 7,      // VMCALL-based SDK call
    SdkHypercall = 8,   // Port-based SDK call (response in extra)
    SchedulerSwitch = 9, // vCPU switch; data = prev_vcpu, extra = quantum
    Intr = 10,           // Host signal (SIGALRM)
    Debug = 11,
    IrqWindowOpen = 12,
    InternalError = 13,
    FaultApplied = 14,   // Fault injected; data = fault_type_id
    InterruptInjected = 15,
    NmiInjected = 16,
    SnapshotTaken = 17,
    SnapshotRestored = 18,
    CoverageSync = 19,  // Coverage bitmap read
    Marker = 255,        // User-defined annotation
}
```

### Writer: BufWriter<File> with 64KB buffer

```rust
struct DlogWriter {
    writer: BufWriter<File>,
    seq: u64,
}
```

Each `emit()` call writes exactly 64 bytes. `BufWriter` batches to
64KB (1024 records) before hitting the kernel. At 500K exits/sec
this is ~490 syscalls/sec — negligible.

Flush on `Drop` and on snapshot/restore boundaries so logs are
complete even if the process crashes.

### Reader and diff

```rust
struct DlogReader {
    mmap: Mmap,  // memmap2 crate
}
```

Read-side uses `memmap2` for zero-copy sequential scan. Each record
is a `&DlogRecord` cast from the mmap slice.

**Diff algorithm:**
1. Open both files, iterate record-by-record.
2. Compare `(tag, exit_count, virtual_tsc, port_or_addr, data)`.
   Skip `rip` by default (can differ due to KASLR if user forgot
   `nokaslr`; add `--strict` flag to include it).
3. On first mismatch, print records `[i-5 .. i+1]` from both logs.
4. Exit code 0 = identical, 1 = divergence found.

### Integration points in vm.rs

The `step()` method's match arms already destructure every exit type.
Each arm gets a one-liner:

```rust
Ok(VcpuExit::IoIn(port, data)) => {
    // ... existing handling ...
    self.dlog_emit(DlogTag::IoIn, port as u32, &data[..]);
    // ...
}
```

`dlog_emit` is an `#[inline]` method:

```rust
#[inline]
fn dlog_emit(&mut self, tag: DlogTag, port_or_addr: u32, data: &[u8]) {
    if let Some(dlog) = &mut self.dlog {
        dlog.emit(DlogRecord { ... });
    }
}
```

The `if let Some` branch is predicted-not-taken when logging is
disabled. On modern x86 this costs 1-2 cycles per exit — lost in
the noise of KVM_RUN's ~2000-cycle overhead.

### CLI

- `chaoscontrol-explore run --dlog <dir>`: creates `<dir>/vm_0.dlog`,
  `<dir>/vm_1.dlog`, etc. One file per VM.
- `chaoscontrol-replay dlog-diff <a.dlog> <b.dlog>`: compare two logs.
- `chaoscontrol-replay dlog-dump <file.dlog> [--from N] [--count M]`:
  human-readable text dump.

### File layout

All in `crates/chaoscontrol-vmm/src/dlog.rs`:
- `DlogRecord`, `DlogTag` (format)
- `DlogWriter` (write path)
- `DlogReader`, `dlog_diff()`, `dlog_dump()` (read path)

CLI wiring in existing binaries.

## Risks / Trade-offs

- **[Disk space]** 100K exits × 64 bytes = 6.4 MB per VM per run.
  Exploration with 200 rounds × 16 branches × 3 VMs = ~60 GB if
  logging every branch. Mitigation: only log the probe branch (or a
  specific seed under investigation), not every exploration branch.
  Make it opt-in per `--dlog` flag.

- **[I/O bandwidth]** 500K exits/sec × 64 B = 32 MB/s per VM. Well
  within NVMe bandwidth but could contend with disk-image I/O on
  spinning rust. Mitigation: BufWriter already batches; users on
  HDD can use `/tmp` (tmpfs).

- **[Record size vs richness]** 64 bytes is tight. Some events (SDK
  hypercall with complex payload, network fault with multiple
  parameters) lose detail. Mitigation: the record captures enough to
  *locate* divergence; once found, replay with `env_logger` at
  TRACE level around that exit range for full detail.

- **[memmap2 dependency]** New crate for the reader. Small, well-
  maintained, widely used. Only needed for the read/diff path, not
  the write-side hot path. Can fall back to buffered `File::read` if
  we want to avoid the dep.
