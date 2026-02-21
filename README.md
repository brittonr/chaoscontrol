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

### VM Infrastructure
- **x86_64 boot**: Full long mode setup with GDT, identity-mapped page
  tables (1 GB via 2 MB pages), and Linux boot protocol support
- **In-kernel IRQ chip**: PIC, IOAPIC, and LAPIC via KVM
- **Serial console**: COM1 with interrupt-driven I/O and output capture
- **Linux kernel support**: Loads ELF kernels via linux-loader

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

## Project Structure

```
chaoscontrol/
├── flake.nix                              # Nix development environment
├── Cargo.toml                             # Workspace root
└── crates/
    ├── chaoscontrol-protocol/             # SDK ↔ VMM wire protocol (no_std)
    │   └── src/lib.rs                     # Hypercall page layout, commands, encoding
    ├── chaoscontrol-sdk/                  # Guest-side SDK (Antithesis-style)
    │   └── src/
    │       ├── lib.rs                     # Public API
    │       ├── assert.rs                  # always, sometimes, reachable, unreachable
    │       ├── lifecycle.rs               # setup_complete, send_event
    │       ├── random.rs                  # Guided randomness (get_random, random_choice)
    │       └── transport.rs               # Hypercall page + I/O port transport
    ├── chaoscontrol-fault/                # Host-side fault injection engine
    │   └── src/
    │       ├── lib.rs
    │       ├── faults.rs                  # Fault types (network, disk, process, clock)
    │       ├── engine.rs                  # FaultEngine: schedule + inject + dispatch
    │       ├── schedule.rs                # Time-based fault scheduling
    │       └── oracle.rs                  # Property oracle (cross-run assertion tracking)
    ├── chaoscontrol-vmm/                  # VMM implementation
    │   └── src/
    │       ├── lib.rs                     # Library root
    │       ├── vm.rs                      # Core VM + SDK hypercall handler
    │       ├── cpu.rs                     # CPUID filtering, TSC, virtual TSC
    │       ├── memory.rs                  # Guest memory, page tables, GDT
    │       ├── snapshot.rs                # VM state snapshot/restore
    │       ├── devices/                   # Deterministic device backends
    │       └── verified/                  # Pure functions for formal verification
    └── chaoscontrol-trace/                # eBPF-based KVM tracing
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

# Run tests (667 unit + doc tests)
cargo test

# Boot a kernel
cargo run --bin boot -- <kernel-path> [initrd-path]

# Snapshot demo
cargo run --release --bin snapshot_demo -- <kernel-path> <initrd-path>

# Exploration — coverage-guided fault schedule search
cargo run --release --bin chaoscontrol-explore -- run \
  --kernel <kernel-path> --initrd <initrd-path> \
  --vms 3 --rounds 200 --branches 16 --output results/

# Exploration with persistent disk image
cargo run --release --bin chaoscontrol-explore -- run \
  --kernel <kernel-path> --initrd <initrd-path> \
  --disk-image <path-to-ext4.img> \
  --vms 3 --rounds 200 --branches 16 --output results/

# Replay — reproduce a recorded session
cargo run --release --bin chaoscontrol-replay -- replay \
  --recording session.json --ticks 5000

# Triage — generate bug report from recording
cargo run --release --bin chaoscontrol-replay -- triage \
  --recording session.json --bug-id 1 --format markdown

# Info — inspect recording metadata
cargo run --release --bin chaoscontrol-replay -- info \
  --recording session.json
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

### Memory Layout (`memory.rs`)

Standard Linux x86_64 boot layout:

```
0x0000_0500  GDT (4 entries)
0x0000_0520  IDT (empty)
0x0000_7000  Zero page (boot_params)
0x0000_8FF0  Boot stack
0x0000_9000  PML4 → PDPTE → PDE (1 GB identity map)
0x0002_0000  Kernel command line
0x0010_0000  HIMEM_START (kernel load, 1 MB)
```

### Deterministic Devices

```rust
use chaoscontrol_vmm::devices::entropy::DeterministicEntropy;
use chaoscontrol_vmm::devices::block::{DeterministicBlock, BlockFault};
use chaoscontrol_vmm::devices::net::DeterministicNet;

// Seeded entropy — same seed = same random bytes
let mut entropy = DeterministicEntropy::new(42);
let val = entropy.next_u64();

// Block device with fault injection
let mut disk = DeterministicBlock::new(1024 * 1024);
disk.write(0, b"hello")?;
disk.inject_fault(BlockFault::TornWrite { offset: 512, bytes_written: 3 });

// Simulated network
let mut net = DeterministicNet::new([0x02, 0x00, 0x00, 0x00, 0x00, 0x01]);
net.inject_packet(vec![/* ethernet frame */]);
let sent = net.drain_tx();
```


### Guest SDK (Antithesis-style)

The `chaoscontrol-sdk` crate provides a guest-side testing API inspired by
[Antithesis](https://antithesis.com). Guest code uses these to annotate
properties and receive guided random values:

```rust
use chaoscontrol_sdk::{assert, lifecycle, random};

// Signal setup complete — faults may begin
lifecycle::setup_complete(&[("nodes", "3")]);

// Safety property: must always hold
assert::always(leader < num_nodes, "valid leader", &[]);

// Liveness property: must hold at least once across all runs
assert::sometimes(write_ok, "write succeeded", &[]);

// Guided random choice for exploration
let action = random::random_choice(3);
```

Communication uses a shared memory page at `0xFE000` (E820 reserved gap)
plus an `outb(0x510)` trigger. The VMM reads the page, dispatches to the
fault engine, and writes the result back.

### Fault Injection Engine

The `chaoscontrol-fault` crate provides host-side chaos engineering:

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
    .build();
```

Supported fault categories: **network** (partition, latency, jitter,
bandwidth, loss, corruption, reorder, duplication), **disk** (I/O errors,
torn writes, corruption, full), **process** (kill, pause, restart),
**clock** (skew, jump), **resource** (memory pressure).
### Run Loop

The VM run loop handles exits and advances the virtual TSC deterministically:

- **IoIn/IoOut**: Serial port I/O, device access
- **Hlt**: VM halted (clean shutdown)
- **Shutdown**: VM shutdown event
- **MmioRead/MmioWrite**: Memory-mapped I/O
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
linux-loader = "0.13"     # Kernel loading (ELF/bzImage)
vm-superio = "0.8"        # Serial port emulation
vmm-sys-util = "0.12"     # EventFd, utilities
rand_chacha = "0.3"       # Seeded PRNG
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
- [x] VMM ↔ SDK hypercall integration
- [x] Virtio transport layer (MMIO-based)
- [x] Wire devices into VM run loop via virtio
- [x] Multi-VM simulation controller
- [x] Deterministic scheduling across VMs
- [x] Coverage feedback from guest (kcov / breakpoints)
- [x] Coverage-guided seed exploration
- [x] Network simulation fidelity (jitter, bandwidth, duplication)
- [x] Kernel coverage (KCOV) — kernel code path visibility for exploration
- [x] Input tree exploration — branch at random_choice() decision points

