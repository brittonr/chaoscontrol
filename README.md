# ChaosControl — Deterministic VMM

A deterministic Virtual Machine Monitor (VMM) for x86_64 built with KVM
and the rust-vmm crate ecosystem. Designed for simulation testing of
distributed systems where reproducibility is essential.


This is just an experiment with Claude + Pi.dev. Use at your own risk

## Features

### Deterministic Execution
- **CPUID filtering**: Comprehensive filtering removes RDRAND, RDSEED,
  RDTSCP, optionally AVX2/AVX-512, and hides hypervisor presence
- **Pinned TSC**: Fixed time stamp counter frequency (default 3.0 GHz)
  for reproducible timing across hosts
- **Virtual TSC**: Software TSC counter that advances only on VM exits,
  enabling fully deterministic time progression
- **Fixed processor identity**: Optional model/family/stepping override
  for cross-host reproducibility
- **SMP support**: Multi-vCPU VMs with serialized execution (Antithesis-style),
  deterministic round-robin or randomized scheduling

### VM Infrastructure
- **x86_64 boot**: Full long mode setup with GDT, identity-mapped page
  tables (1 GB via 2 MB pages), and Linux boot protocol support
- **In-kernel IRQ chip**: PIC, IOAPIC, and LAPIC via KVM
- **Serial console**: COM1 with interrupt-driven I/O and output capture
- **Linux kernel support**: Loads ELF kernels via linux-loader
- **ACPI tables**: RSDP/RSDT/MADT for SMP CPU topology

### Snapshot / Restore
- **Complete state capture**: CPU registers, FPU, debug registers, LAPIC,
  XCRs, IRQ chip (PIC master/slave, IOAPIC), PIT, KVM clock, and full
  guest memory
- **Instant restore**: Resume execution from any captured checkpoint
- **Fork support**: Create divergent execution paths from a single
  snapshot point
- **Copy-on-write block device**: Snapshots share the base disk image
  via `Arc`; only dirty 4 KB pages are cloned — a 512 MB disk with 1 MB
  of writes costs ~1 MB per snapshot, not 512 MB

### Deterministic Devices
- **Entropy**: Seeded ChaCha20 PRNG replacing hardware RNG, with
  snapshot/restore and reseed for exploration
- **Block**: Copy-on-write block device with optional disk image file
  backing (`--disk-image`). Supports fault injection (read errors,
  write errors, torn writes, corruption)
- **Network**: Simulated network with RX/TX queues, latency, jitter,
  bandwidth limiting, packet loss/corruption/reorder/duplication for
  fully controlled packet delivery between VMs

### Exploration & Bug Finding
- **Coverage-guided exploration**: AFL-style edge coverage bitmaps,
  fork-from-snapshot branching, frontier-based search
- **Three exploration modes**: fault-schedule mutation, input-tree
  branching at `random_choice()` points, or hybrid
- **Fault schedule minimization**: Delta debugging (ddmin) to find
  the smallest schedule that triggers a bug
- **Bug reproduction**: Replay a bug report to verify it still triggers
- **Assertion catalog**: Compile-time registration of all assertion sites
  via `linkme`; reports show which assertions are exercised/unexercised
- **Per-round history**: Coverage growth curves, plateau detection,
  bug discovery timeline

### Determinism Logging
- **Binary dlog format**: Per-exit event log for diagnosing non-determinism
- **Structural diff**: Compare two runs ignoring data payloads
- **Register dumps**: Periodic full-register snapshots in dlog
- **Memory hashing**: CRC32 page hashes at snapshot boundaries

## Project Structure

```
chaoscontrol/
├── flake.nix                              # Nix development environment
├── Cargo.toml                             # Workspace root
└── crates/
    ├── chaoscontrol-protocol/             # SDK ↔ VMM wire protocol (no_std)
    ├── chaoscontrol-sdk/                  # Guest-side SDK (Antithesis-style)
    ├── chaoscontrol-fault/                # Host-side fault injection engine
    ├── chaoscontrol-vmm/                  # VMM implementation
    ├── chaoscontrol-explore/              # Coverage-guided exploration engine
    ├── chaoscontrol-replay/               # Recording, replay, time-travel debugger
    ├── chaoscontrol-trace/                # eBPF-based KVM tracing
    ├── chaoscontrol-guest/                # Minimal SDK-instrumented guest binary
    ├── chaoscontrol-raft-guest/           # 3-node Raft consensus guest (35 assertions)
    ├── chaoscontrol-guest-net/            # Network guest library (smoltcp)
    └── chaoscontrol-net-guest/            # Network demo guest binary
```

### Kernel Coverage (KCOV)

When the guest kernel is built with `CONFIG_KCOV=y`, the SDK
automatically collects kernel code coverage and merges it into the
same AFL-style bitmap used by userspace SanCov.  This gives the
explorer visibility into kernel code paths exercised by different
fault schedules — filesystem error handling, network stack branches,
scheduler decisions, etc.

```bash
# Build KCOV-enabled kernel (first time takes ~20 min)
nix build .#kcov-vmlinux -o result-kcov

# Run exploration with kernel coverage
cargo run --release --bin chaoscontrol-explore -- run \
  --kernel result-kcov/vmlinux --initrd guest/initrd-raft.gz \
  --vms 3 --rounds 200 --branches 16

# Guest SDK auto-detects KCOV — no code changes needed
```

On a standard kernel (without `CONFIG_KCOV`), the SDK gracefully
falls back to userspace-only coverage — no crash, no error.

## Building

```bash
# Enter development environment
nix develop

# Build
cargo build

# Run tests (827 unit + doc tests)
cargo test

# Boot a kernel
cargo run --bin boot -- <kernel-path> [initrd-path]

# Snapshot demo
cargo run --release --bin snapshot_demo -- <kernel-path> <initrd-path>
```

## CLI Tools

### Exploration

```bash
# Coverage-guided exploration
cargo run --release --bin chaoscontrol-explore -- run \
  --kernel <kernel-path> --initrd <initrd-path> \
  --vms 3 --rounds 200 --branches 16 --output results/

# With persistent disk image
cargo run --release --bin chaoscontrol-explore -- run \
  --kernel <kernel-path> --initrd <initrd-path> \
  --disk-image <path-to-ext4.img> \
  --vms 3 --rounds 200 --branches 16 --output results/

# Input-tree mode (branch at random_choice() points)
cargo run --release --bin chaoscontrol-explore -- run \
  --kernel <kernel-path> --initrd <initrd-path> \
  --mode input-tree --output results/

# Resume from checkpoint
cargo run --release --bin chaoscontrol-explore -- resume \
  --corpus results/ --rounds 500
```

Output directory contains:
- `checkpoint.json` — resumable exploration state
- `report.txt` — human-readable report with per-round history
- `assertions.json` — per-assertion verdicts and hit counts
- `bug_N.json` — bug reports (consumable by minimize/reproduce)

### Bug Workflow

```bash
# 1. Explore — find bugs
cargo run --release --bin chaoscontrol-explore -- run \
  --kernel vmlinux --initrd initrd.gz \
  --vms 3 --rounds 100 --output results/

# 2. Minimize — shrink the fault schedule
cargo run --release --bin chaoscontrol-explore -- minimize \
  --kernel vmlinux --initrd initrd.gz \
  --bug results/bug_0.json --output minimized.json

# 3. Reproduce — verify the bug
cargo run --release --bin chaoscontrol-explore -- reproduce \
  --kernel vmlinux --initrd initrd.gz \
  --bug minimized.json --serial
```

### Replay & Debugging

```bash
# Replay a recorded session
cargo run --release --bin chaoscontrol-replay -- replay \
  --recording session.json --ticks 5000

# Triage — generate bug report from recording
cargo run --release --bin chaoscontrol-replay -- triage \
  --recording session.json --bug-id 1 --format markdown

# Show recording metadata
cargo run --release --bin chaoscontrol-replay -- info \
  --recording session.json

# Determinism log tools
cargo run --release --bin chaoscontrol-replay -- dlog diff a.dlog b.dlog
cargo run --release --bin chaoscontrol-replay -- dlog dump run.dlog
cargo run --release --bin chaoscontrol-replay -- dlog stats run.dlog
```

### eBPF Tracing

```bash
# Live KVM trace (requires sudo)
sudo chaoscontrol-trace live --pid <VMM_PID> --output trace.json

# Verify determinism between two traces
chaoscontrol-trace verify --trace-a run1.json --trace-b run2.json
```

## Architecture

### VM Setup (`vm.rs`)

`DeterministicVm` is the main entry point, configured via `VmConfig`:

```rust
use chaoscontrol_vmm::vm::{DeterministicVm, VmConfig};
use chaoscontrol_vmm::cpu::CpuConfig;

let config = VmConfig {
    memory_size: 256 * 1024 * 1024,
    cpu: CpuConfig {
        tsc_khz: 3_000_000,
        seed: 42,
        ..CpuConfig::default()
    },
    ..VmConfig::default()
};

let mut vm = DeterministicVm::new(config)?;
vm.load_kernel("vmlinux", Some("initrd.gz"))?;
vm.run()?;
```

### CPU Determinism (`cpu.rs`)

Comprehensive CPUID filtering:

| CPUID Leaf | What's Filtered | Why |
|------------|----------------|-----|
| 0x1 | RDRAND, TSC-Deadline, hypervisor bit | Hardware RNG, timer jitter |
| 0x7 | RDSEED, AVX2, AVX-512 | Hardware RNG, ISA variation |
| 0x15 | TSC frequency info | Fixed crystal clock ratio |
| 0x16 | Processor frequency | Consistent MHz reporting |
| 0x40000000+ | KVM paravirt leaves | Hide hypervisor presence |
| 0x80000001 | RDTSCP | Bypasses MSR-trap path |
| 0x80000007 | Invariant TSC | Guest shouldn't assume host TSC |

Virtual TSC for fully deterministic time:

```rust
use chaoscontrol_vmm::cpu::VirtualTsc;

let mut vtsc = VirtualTsc::new(3_000_000, 1_000);
vtsc.tick();                    // Advance by 1000 counts
let ns = vtsc.elapsed_ns();    // Convert to nanoseconds
let snap = vtsc.snapshot();    // Serialize for checkpoints
```

### Guest SDK (Antithesis-style)

The `chaoscontrol-sdk` crate provides a guest-side testing API inspired by
[Antithesis](https://antithesis.com). Guest code uses these to annotate
properties and receive guided random values:

```rust
use chaoscontrol_sdk::prelude::*;

chaoscontrol_init();

// Signal setup complete — faults may begin
lifecycle::setup_complete(&[("nodes", "3")]);

// Safety property: must always hold
cc_assert_always!(leader < num_nodes, "valid leader");

// Liveness property: must hold at least once across all runs
cc_assert_sometimes!(write_ok, "write succeeded");

// Reachability
cc_assert_reachable!("leader elected");
cc_assert_unreachable!("split brain");

// Guided random choice for exploration
let action = random::random_choice(3);
```

All assertion sites are registered at compile time via `linkme` and
reported to the VMM at startup. The exploration report shows which
assertions were exercised, passed, failed, or never reached.

### Fault Injection Engine

```rust
use chaoscontrol_fault::schedule::FaultScheduleBuilder;
use chaoscontrol_fault::faults::Fault;

let schedule = FaultScheduleBuilder::new()
    .at_ns(1_000_000_000, Fault::NetworkPartition {
        side_a: vec![0],
        side_b: vec![1, 2],
    })
    .at_ns(5_000_000_000, Fault::NetworkHeal)
    .at_ns(8_000_000_000, Fault::ProcessKill { target: 1 })
    .at_ns(10_000_000_000, Fault::InjectInterrupt { target: 0, irq: 5 })
    .build();
```

**27 fault types** across 6 categories: network (partition, latency,
jitter, bandwidth, loss, corruption, reorder, duplication, heal), disk
(I/O errors, torn writes, corruption, full), process (kill, pause,
restart), clock (skew, jump), resource (memory pressure), interrupt
(IRQ injection, NMI).

### Run Loop

The VM run loop handles exits and advances the virtual TSC deterministically:

- **IoIn/IoOut**: Serial port I/O, device access, SDK hypercalls
- **Hlt**: VM halted — fast-forward TSC + inject timer IRQ
- **MmioRead/MmioWrite**: Virtio MMIO, HPET, ACPI PM timer
- **Hypercall**: VMCALL-based SDK transport (preferred over port I/O)
- Every exit increments the virtual TSC by a fixed amount

Execution modes:
- `run()` — run until halt/shutdown
- `run_until(pattern)` — run until serial output matches
- `run_bounded(max_exits)` — run for N exits (deterministic scheduling)

## Dependencies

```toml
kvm-ioctls = "0.19"       # KVM API
kvm-bindings = "0.10"     # KVM structures
vm-memory = "0.17"        # Guest memory management
linux-loader = "0.13"     # Kernel loading (ELF)
vm-superio = "0.8"        # Serial port emulation
vmm-sys-util = "0.12"     # EventFd, utilities
rand_chacha = "0.3"       # Seeded PRNG
linkme = "0.3"            # Compile-time assertion catalog
snafu = "0.8"             # Error handling
```

## Roadmap

- [x] Boot Linux kernel in single-vCPU KVM VM
- [x] CPUID filtering (RDRAND, RDSEED, RDTSCP, AVX, hypervisor)
- [x] TSC pinning + virtual TSC tracking
- [x] Complete snapshot/restore (CPU + memory + devices)
- [x] Deterministic entropy (seeded ChaCha20)
- [x] Deterministic block device with fault injection
- [x] Deterministic network (simulated queues)
- [x] Guest SDK (Antithesis-style assertions + guided randomness)
- [x] Fault injection engine (network, disk, process, clock faults)
- [x] Property oracle (cross-run assertion tracking + verdicts)
- [x] VMM ↔ SDK hypercall integration (VMCALL + port I/O fallback)
- [x] Virtio transport layer (MMIO-based, blk + net + rng)
- [x] Multi-VM simulation controller with network fabric
- [x] Deterministic scheduling across VMs
- [x] SMP — multi-vCPU with serialized execution
- [x] Coverage-guided exploration (AFL-style edge bitmaps)
- [x] Input tree exploration — branch at random_choice() decision points
- [x] Network simulation fidelity (jitter, bandwidth, duplication)
- [x] Kernel coverage (KCOV) — kernel code path visibility
- [x] Assertion catalog — compile-time registration via linkme
- [x] Fault schedule minimization — delta debugging
- [x] Bug reproduction from JSON reports
- [x] Determinism logging (dlog) — binary event log + diff + stats
- [x] Time-travel debugger with counterfactual analysis
- [x] Per-round exploration history and plateau detection
- [x] Per-assertion detail reports with JSON export
- [x] Multi-VM networking (virtio-net + smoltcp TCP/IP)
- [x] Interrupt injection faults (IRQ + NMI)
- [x] Core pinning for reduced scheduling jitter
