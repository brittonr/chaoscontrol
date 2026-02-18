# ChaosControl — Deterministic VMM

A deterministic Virtual Machine Monitor (VMM) for x86_64 built with KVM
and the rust-vmm crate ecosystem. Designed for simulation testing of
distributed systems where reproducibility is essential.

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

### Deterministic Devices
- **Entropy**: Seeded ChaCha20 PRNG replacing hardware RNG, with
  snapshot/restore and reseed for exploration
- **Block**: In-memory block device with fault injection (read errors,
  write errors, torn writes, corruption)
- **Network**: Simulated network with RX/TX queues for fully controlled
  packet delivery between VMs

## Project Structure

```
chaoscontrol/
├── flake.nix                          # Nix development environment
├── Cargo.toml                         # Workspace root
└── crates/
    └── chaoscontrol-vmm/              # VMM implementation
        ├── Cargo.toml
        └── src/
            ├── lib.rs                 # Library root
            ├── vm.rs                  # Core VM: create, boot, run loop
            ├── cpu.rs                 # CPUID filtering, TSC, virtual TSC
            ├── memory.rs              # Guest memory, page tables, GDT
            ├── snapshot.rs            # VM state snapshot/restore
            ├── devices/
            │   ├── mod.rs
            │   ├── serial.rs          # Serial port constants
            │   ├── entropy.rs         # Seeded PRNG entropy source
            │   ├── block.rs           # In-memory block device + faults
            │   └── net.rs             # Simulated network queues
            └── bin/
                ├── boot.rs            # Boot a Linux kernel
                └── snapshot_demo.rs   # Snapshot/restore demonstration
```

## Building

```bash
# Enter development environment
nix develop

# Build
cargo build

# Run tests (105 unit + 11 doc-tests)
cargo test

# Boot a kernel
cargo run --bin boot -- <kernel-path> [initrd-path]

# Snapshot demo
cargo run --release --bin snapshot_demo -- <kernel-path> <initrd-path>
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
- [ ] Virtio transport layer (MMIO-based)
- [ ] Wire devices into VM run loop via virtio
- [ ] Multi-VM simulation controller
- [ ] Deterministic scheduling across VMs
- [ ] Coverage feedback from guest (kcov / breakpoints)
- [ ] Fault injection engine (partitions, delays, kills)

## License

Apache-2.0
