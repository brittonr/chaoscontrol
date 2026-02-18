# ChaosControl: Deterministic VMM for Simulation Testing

## Why rust-vmm, Not Firecracker or Cloud Hypervisor

You already have madsim for pure simulation. You need the **next layer**: run
the actual production binary, catch real bugs, snapshot/fork mid-execution.

The VMM landscape looks like this:

```
rust-vmm crates (building blocks)
├── kvm-ioctls       ← KVM API: create VM, run vCPU, set registers
├── kvm-bindings     ← KVM data structures
├── vm-memory        ← Guest memory management
├── linux-loader     ← Load kernel/initrd into guest memory
├── vm-superio       ← Serial port, i8042 keyboard controller
└── vmm-sys-util     ← EventFd, signal handling, terminal
         │
         ├── Firecracker (115K lines)  ← Amazon Lambda VMM
         │     Lots of stuff you don't need:
         │     rate limiters, API server, jailer,
         │     balloon device, hotplug, seccomp,
         │     ACPI tables, multi-vCPU threading...
         │
         ├── Cloud Hypervisor (133K lines)  ← Even more stuff
         │     PCI bus, vhost-user, VFIO, TDX,
         │     live migration, NUMA, huge pages...
         │
         └── ChaosControl (your VMM, ~5K lines)  ← JUST what you need
               Single vCPU, deterministic clock,
               controlled I/O, fast snapshot/fork,
               fault injection, seeded entropy
```

**Firecracker/Cloud Hypervisor are general-purpose VMMs.** You'd spend more
time ripping out what you don't need than building what you do. A
purpose-built deterministic VMM on rust-vmm is ~5K lines because you
skip everything irrelevant.

## What rust-vmm Gives You

The `kvm-ioctls` crate provides direct access to every KVM capability
you need for determinism:

```rust
// Everything you need, nothing you don't
vcpu.set_cpuid2(&cpuid)           // Filter CPUID (disable RDRAND, AVX, etc.)
vcpu.set_tsc_khz(freq)            // Pin TSC frequency
vcpu.set_regs(&regs)              // Set/get all registers
vcpu.set_sregs(&sregs)            // Control segments, CR3, etc.
vcpu.get_msrs(&mut msrs)          // Read/write MSRs
vcpu.set_msrs(&msrs)              // Set IA32_TSC, etc.
vcpu.set_guest_debug(&debug)      // Single-stepping, breakpoints
vcpu.run()                        // Run until VM exit

vm.set_msr_filter(DENY, &ranges)  // Trap specific MSR reads → VM exit
vm.set_clock(&clock_data)         // Control KVM clock
vm.get_clock()                    // Snapshot clock state
vm.create_irq_chip()              // In-kernel APIC
vm.set_irqchip(&chip)             // Snapshot/restore APIC state
vm.get_pit2()                     // Snapshot PIT timer
vm.set_pit2(&pit)                 // Restore PIT timer
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        ChaosControl                              │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │                   Exploration Engine                        │  │
│  │  • Seed-based scheduling decisions                         │  │
│  │  • Snapshot at interesting states                          │  │
│  │  • Fork & explore different fault scenarios                │  │
│  │  • Coverage feedback from guest (kcov / breakpoints)       │  │
│  └─────────────────────────┬──────────────────────────────────┘  │
│                            │                                     │
│  ┌─────────────────────────▼──────────────────────────────────┐  │
│  │                Fault Injection Engine                       │  │
│  │  • Network: partition, delay, reorder, corrupt, drop       │  │
│  │  • Disk: error, torn write, full, slow                     │  │
│  │  • Process: kill, pause, OOM                               │  │
│  │  • Clock: skew, jump                                       │  │
│  └─────────────────────────┬──────────────────────────────────┘  │
│                            │                                     │
│  ┌─────────────────────────▼──────────────────────────────────┐  │
│  │               Deterministic VMM Core                        │  │
│  │                                                             │  │
│  │  ┌──────────────┐ ┌──────────────┐ ┌────────────────────┐  │  │
│  │  │  Single vCPU  │ │  Det. Clock  │ │ Snapshot Manager   │  │  │
│  │  │  (serialized  │ │  (pinned TSC │ │ (memory + regs +   │  │  │
│  │  │   run loop)   │ │   + MSR trap)│ │  device state)     │  │  │
│  │  └──────────────┘ └──────────────┘ └────────────────────┘  │  │
│  │                                                             │  │
│  │  ┌──────────────┐ ┌──────────────┐ ┌────────────────────┐  │  │
│  │  │  Det. Net    │ │  Det. Block  │ │ Det. Entropy       │  │  │
│  │  │  (virtio-net │ │  (virtio-blk │ │ (virtio-rng with   │  │  │
│  │  │   with sim   │ │   with sim   │ │  seeded PRNG)      │  │  │
│  │  │   backend)   │ │   backend)   │ │                    │  │  │
│  │  └──────────────┘ └──────────────┘ └────────────────────┘  │  │
│  │                                                             │  │
│  │         Built on: kvm-ioctls, vm-memory, linux-loader       │  │
│  └─────────────────────────────────────────────────────────────┘  │
│                            │                                     │
│           ┌────────────────┼────────────────┐                    │
│           ▼                ▼                ▼                    │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐            │
│  │   Guest VM    │ │   Guest VM    │ │   Guest VM    │           │
│  │              │ │              │ │              │            │
│  │  Mini Linux  │ │  Mini Linux  │ │  Mini Linux  │            │
│  │  + your svc  │ │  + your svc  │ │  + your svc  │            │
│  │              │ │              │ │              │            │
│  │  (actual     │ │  (actual     │ │  (actual     │            │
│  │   prod bin)  │ │   prod bin)  │ │   prod bin)  │            │
│  └──────────────┘ └──────────────┘ └──────────────┘            │
└─────────────────────────────────────────────────────────────────┘
```

## The Deterministic VMM Core (~2-3K lines)

### Boot Sequence

```rust
use kvm_ioctls::{Kvm, VmFd, VcpuFd, VcpuExit};
use vm_memory::{GuestMemoryMmap, GuestAddress};
use linux_loader::loader::KernelLoader;

pub struct DeterministicVm {
    vm_fd: VmFd,
    vcpu_fd: VcpuFd,
    guest_memory: GuestMemoryMmap,

    // Determinism control
    seed: u64,
    rng: ChaCha20Rng,
    virtual_tsc: u64,
    tsc_khz: u32,

    // Device backends (deterministic)
    net: DeterministicNet,
    block: DeterministicBlock,
    entropy: DeterministicEntropy,
    serial: Serial,

    // Snapshot state
    snapshot_memory: Option<Vec<u8>>,
    snapshot_regs: Option<SavedState>,
}

impl DeterministicVm {
    pub fn new(config: VmConfig) -> Result<Self> {
        let kvm = Kvm::new()?;
        let vm_fd = kvm.create_vm()?;
        let vcpu_fd = vm_fd.create_vcpu(0)?;

        // 1. Set up guest memory
        let guest_memory = GuestMemoryMmap::from_ranges(&[
            (GuestAddress(0), config.mem_size),
        ])?;
        let mem_region = kvm_userspace_memory_region { /* ... */ };
        unsafe { vm_fd.set_user_memory_region(mem_region)? };

        // 2. Load kernel + initrd
        linux_loader::loader::elf::Elf::load(
            &guest_memory,
            None,  // kernel_offset
            &mut File::open(&config.kernel_path)?,
            None,  // highmem_start_address
        )?;

        // 3. DETERMINISM: Filter CPUID
        let mut cpuid = kvm.get_supported_cpuid(KVM_MAX_CPUID_ENTRIES)?;
        Self::make_deterministic_cpuid(&mut cpuid, &config);
        vcpu_fd.set_cpuid2(&cpuid)?;

        // 4. DETERMINISM: Pin TSC
        vcpu_fd.set_tsc_khz(config.tsc_khz)?;

        // 5. DETERMINISM: MSR filter — trap reads to time-related MSRs
        // This makes reads to IA32_TSC cause a VM exit we handle
        vm_fd.set_msr_filter(
            MsrFilterDefaultAction::ALLOW,  // Allow most MSRs
            &[MsrFilterRange {
                flags: MsrFilterRangeFlags::READ,
                base: 0x10,       // IA32_TSC
                msr_count: 1,
                bitmap: &[0x00],  // 0 = deny (trap to VMM)
            }],
        )?;

        // 6. Set up in-kernel interrupt controller
        vm_fd.create_irq_chip()?;
        vm_fd.create_pit2(kvm_pit_config::default())?;

        // 7. Set up deterministic device backends
        let rng = ChaCha20Rng::seed_from_u64(config.seed);

        Ok(Self {
            vm_fd, vcpu_fd, guest_memory,
            seed: config.seed,
            rng,
            virtual_tsc: 0,
            tsc_khz: config.tsc_khz,
            net: DeterministicNet::new(config.seed),
            block: DeterministicBlock::new(&config.disk_image),
            entropy: DeterministicEntropy::new(config.seed),
            serial: Serial::new(),
            snapshot_memory: None,
            snapshot_regs: None,
        })
    }

    fn make_deterministic_cpuid(cpuid: &mut CpuId, config: &VmConfig) {
        for entry in cpuid.as_mut_slice() {
            match entry.function {
                0x1 => {
                    entry.ecx &= !(1 << 30); // Disable RDRAND
                    entry.ecx &= !(1 << 24); // Disable TSC-Deadline
                    // Optionally disable hypervisor bit to hide KVM
                    // entry.ecx &= !(1 << 31);
                }
                0x7 if entry.index == 0 => {
                    entry.ebx &= !(1 << 18); // Disable RDSEED
                    if !config.allow_avx {
                        entry.ebx &= !(1 << 5);  // Disable AVX2
                        entry.ebx &= !(1 << 16); // Disable AVX-512F
                    }
                }
                0x80000001 => {
                    // Disable RDTSCP (AMD)
                    entry.edx &= !(1 << 27);
                }
                0x80000007 => {
                    // Hide invariant TSC advertisement
                    entry.edx &= !(1 << 8);
                }
                _ => {}
            }
        }
    }
}
```

### The Run Loop — Where Determinism Happens

```rust
impl DeterministicVm {
    /// Run the VM. All non-determinism is intercepted here.
    pub fn run_until_event(&mut self) -> Result<VmEvent> {
        loop {
            match self.vcpu_fd.run()? {
                VcpuExit::Hlt => return Ok(VmEvent::Halted),

                VcpuExit::IoOut(port, data) => {
                    // Guest wrote to an I/O port — dispatch to device
                    self.handle_io_out(port, data)?;
                }
                VcpuExit::IoIn(port, data) => {
                    // Guest reading from I/O port
                    self.handle_io_in(port, data)?;
                }

                VcpuExit::MmioWrite(addr, data) => {
                    // Virtio MMIO device access
                    self.handle_mmio_write(addr, data)?;
                }
                VcpuExit::MmioRead(addr, data) => {
                    self.handle_mmio_read(addr, data)?;
                }

                // KEY: MSR filter traps IA32_TSC reads
                VcpuExit::X86Rdmsr(exit) => {
                    match exit.index {
                        0x10 => {
                            // IA32_TSC — return our deterministic value
                            *exit.data = self.virtual_tsc;
                            *exit.error = 0; // Success
                            self.virtual_tsc += 1000; // Advance by fixed amount
                        }
                        _ => {
                            *exit.error = 1; // Deny unknown MSRs
                        }
                    }
                }

                VcpuExit::Shutdown => return Ok(VmEvent::Shutdown),
                VcpuExit::InternalError => return Ok(VmEvent::Error),

                // EINTR from our signal handler — check if we requested pause
                _ => continue,
            }
        }
    }
}
```

### Snapshot / Fork

```rust
/// Everything needed to restore a VM to an exact state
#[derive(Clone, Serialize, Deserialize)]
pub struct VmSnapshot {
    // CPU state
    regs: kvm_regs,
    sregs: kvm_sregs,
    fpu: kvm_fpu,
    msrs: Vec<kvm_msr_entry>,
    debug_regs: kvm_debugregs,
    lapic: kvm_lapic_state,
    xcrs: kvm_xcrs,
    xsave: kvm_xsave,
    tsc_khz: u32,

    // Memory (use copy-on-write for efficiency)
    memory: Vec<u8>,  // or use userfaultfd + CoW pages

    // Kernel devices
    irqchip: kvm_irqchip,
    pit: kvm_pit_state2,
    clock: kvm_clock_data,

    // Determinism state
    virtual_tsc: u64,
    rng_state: [u8; 32],  // ChaCha20 state
    seed: u64,

    // Device state
    net_state: DeterministicNetState,
    block_state: DeterministicBlockState,
    entropy_state: DeterministicEntropyState,
}

impl DeterministicVm {
    pub fn snapshot(&self) -> Result<VmSnapshot> {
        Ok(VmSnapshot {
            regs: self.vcpu_fd.get_regs()?,
            sregs: self.vcpu_fd.get_sregs()?,
            fpu: self.vcpu_fd.get_fpu()?,
            msrs: self.get_all_msrs()?,
            debug_regs: self.vcpu_fd.get_debug_regs()?,
            lapic: self.vcpu_fd.get_lapic()?,
            xcrs: self.vcpu_fd.get_xcrs()?,
            xsave: self.vcpu_fd.get_xsave()?,
            tsc_khz: self.tsc_khz,

            memory: self.guest_memory.dump()?,

            irqchip: self.vm_fd.get_irqchip(/* PIC/IOAPIC */)?,
            pit: self.vm_fd.get_pit2()?,
            clock: self.vm_fd.get_clock()?,

            virtual_tsc: self.virtual_tsc,
            rng_state: self.rng.get_state(),
            seed: self.seed,

            net_state: self.net.snapshot(),
            block_state: self.block.snapshot(),
            entropy_state: self.entropy.snapshot(),
        })
    }

    /// Fork: create a new VM from a snapshot with a different seed
    pub fn fork(snapshot: &VmSnapshot, new_seed: u64) -> Result<Self> {
        let mut vm = Self::from_snapshot(snapshot)?;
        // Reseed the RNG — same snapshot, different exploration path
        vm.rng = ChaCha20Rng::seed_from_u64(new_seed);
        // Keep same virtual_tsc — time continues from snapshot point
        Ok(vm)
    }

    fn from_snapshot(snapshot: &VmSnapshot) -> Result<Self> {
        let kvm = Kvm::new()?;
        let vm_fd = kvm.create_vm()?;
        let vcpu_fd = vm_fd.create_vcpu(0)?;

        // Restore memory
        let guest_memory = GuestMemoryMmap::from_snapshot(&snapshot.memory)?;
        // ... map into KVM ...

        // Restore CPU
        vcpu_fd.set_regs(&snapshot.regs)?;
        vcpu_fd.set_sregs(&snapshot.sregs)?;
        vcpu_fd.set_fpu(&snapshot.fpu)?;
        vcpu_fd.set_msrs(&snapshot.msrs)?;
        vcpu_fd.set_debug_regs(&snapshot.debug_regs)?;
        vcpu_fd.set_lapic(&snapshot.lapic)?;
        vcpu_fd.set_xcrs(&snapshot.xcrs)?;
        // ... xsave, cpuid, tsc ...

        // Restore kernel devices
        vm_fd.set_irqchip(&snapshot.irqchip)?;
        vm_fd.set_pit2(&snapshot.pit)?;
        vm_fd.set_clock(&snapshot.clock)?;

        // Restore determinism state
        // ... net, block, entropy from snapshot ...

        Ok(vm)
    }
}
```

## Deterministic Device Backends (~1-2K lines)

Instead of real virtio devices talking to the host network/disk, you
build backends that are fully deterministic and controllable.

### Deterministic Network

```rust
/// Replaces virtio-net's TAP backend with a simulated network
pub struct DeterministicNet {
    /// Packets queued for delivery to this VM
    rx_queue: VecDeque<Vec<u8>>,
    /// Packets sent by this VM (consumed by simulation controller)
    tx_queue: VecDeque<Vec<u8>>,
    /// The virtio-net device itself (handles virtqueue protocol)
    device: VirtioNetDevice,
}

impl DeterministicNet {
    /// Called by the simulation controller to deliver a packet
    pub fn inject_packet(&mut self, data: Vec<u8>) {
        self.rx_queue.push_back(data);
        // Signal the virtio interrupt so guest processes it
    }

    /// Called by the simulation controller to drain sent packets
    pub fn drain_tx(&mut self) -> Vec<Vec<u8>> {
        self.tx_queue.drain(..).collect()
    }
}
```

### Deterministic Entropy

```rust
/// Replaces virtio-rng's real entropy with seeded PRNG
pub struct DeterministicEntropy {
    rng: ChaCha20Rng,
}

impl DeterministicEntropy {
    pub fn fill_buffer(&mut self, buf: &mut [u8]) {
        self.rng.fill_bytes(buf);
    }
}
```

### Deterministic Block

```rust
/// In-memory block device with fault injection
pub struct DeterministicBlock {
    /// Disk contents in memory (or CoW over a base image)
    data: Vec<u8>,
    /// Pending fault injections
    faults: VecDeque<BlockFault>,
}

pub enum BlockFault {
    ReadError { sector: u64 },
    WriteError { sector: u64 },
    TornWrite { sector: u64, bytes_written: usize },
    Slow { sector: u64, delay_ticks: u64 },
}
```

## Multi-VM Simulation Controller (~1K lines)

```rust
/// Orchestrates multiple VMs as a simulated distributed system
pub struct Simulation {
    vms: Vec<DeterministicVm>,
    network: SimulatedNetwork,
    clock: VirtualClock,
    fault_schedule: Vec<ScheduledFault>,
    seed: u64,
    rng: ChaCha20Rng,
}

impl Simulation {
    /// Main simulation loop
    pub fn run(&mut self, max_steps: u64) -> SimResult {
        for step in 0..max_steps {
            // Run each VM for a bounded number of exits
            // Order is deterministic (based on seed)
            let order = self.rng.shuffle(0..self.vms.len());
            for &vm_idx in &order {
                let vm = &mut self.vms[vm_idx];
                for _ in 0..EXITS_PER_STEP {
                    match vm.run_until_event()? {
                        VmEvent::Halted => break,
                        VmEvent::NetworkTx(pkt) => {
                            self.network.route(vm_idx, pkt);
                        }
                        _ => {}
                    }
                }
            }

            // Deliver network packets (deterministic order)
            self.network.deliver_ready(&mut self.vms, &self.clock);

            // Maybe inject faults
            self.maybe_inject_fault(step);

            // Check invariants
            self.check_properties()?;
        }
        SimResult::Passed
    }
}
```

## What About the Guest Kernel?

You need a **minimal Linux kernel** tuned for determinism. NOT a custom
kernel — just a specifically configured one:

```bash
# Minimal kernel config for deterministic simulation
CONFIG_SMP=n                    # Single CPU — no SMP scheduling
CONFIG_HZ_100=y                 # Slow timer tick (less jitter)
CONFIG_NO_HZ_IDLE=n             # Keep periodic tick (more predictable)
CONFIG_PREEMPT_NONE=y           # No kernel preemption
CONFIG_RANDOMIZE_BASE=n         # No KASLR
CONFIG_STACKPROTECTOR=n         # Deterministic stack layout
CONFIG_VIRTIO=y                 # Our device model
CONFIG_VIRTIO_NET=y
CONFIG_VIRTIO_BLK=y
CONFIG_HW_RANDOM_VIRTIO=y      # We control this
CONFIG_RANDOM_TRUST_CPU=n       # Don't trust RDRAND (we disabled it)
CONFIG_INIT_ON_ALLOC_DEFAULT_ON=y  # Zero-init allocations
```

Boot parameters:
```
nokaslr                         # Disable ASLR
randomize_kstack_offset=off     # Deterministic stack offsets  
norandmaps                      # Disable mmap randomization
clocksource=tsc tsc=reliable    # Use our controlled TSC
nosmp                           # Single CPU
```

This gets you ~95% deterministic for single-threaded guest workloads.
The remaining non-determinism comes from interrupt timing, which you
control via the PIT/APIC snapshot and the rate at which you inject
virtio interrupts.

## How This Relates to Madsim

```
              Bugs madsim finds          Bugs ChaosControl finds
              ┌─────────────────┐        ┌─────────────────────┐
              │ Logic bugs      │        │ Logic bugs          │
              │ Protocol bugs   │        │ Protocol bugs       │
              │ Race conditions │        │ Race conditions     │
              │ (in sim build)  │        │ (in PROD build)     │
              │                 │        │ unsafe/FFI bugs     │
              │                 │        │ Allocator bugs      │
              │                 │        │ Real tokio bugs     │
              │                 │        │ Kernel interaction  │
              │                 │        │ Actual binary bugs  │
              └─────────────────┘        └─────────────────────┘
                    Fast                      Slower
                    ~1000 tests/sec           ~10 tests/sec
                    In-process                VM per node
                    Sim build                 Prod build

         Use BOTH: madsim for fast iteration, ChaosControl for
         high-fidelity validation of the actual production artifact.
```

## Concrete Crate Structure

```
chaoscontrol/
├── Cargo.toml
├── crates/
│   ├── chaoscontrol-vmm/          # The deterministic VMM core
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── vm.rs              # DeterministicVm
│   │   │   ├── cpu.rs             # CPUID filtering, TSC, MSR traps
│   │   │   ├── memory.rs          # Guest memory + snapshot
│   │   │   ├── devices/
│   │   │   │   ├── net.rs         # Deterministic virtio-net backend
│   │   │   │   ├── block.rs       # Deterministic virtio-blk backend
│   │   │   │   ├── entropy.rs     # Seeded virtio-rng
│   │   │   │   └── serial.rs      # Console output capture
│   │   │   └── snapshot.rs        # Full VM snapshot/restore
│   │   └── Cargo.toml             # deps: kvm-ioctls, vm-memory, linux-loader
│   │
│   ├── chaoscontrol-sim/          # Multi-VM simulation controller
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── simulation.rs      # Orchestrator
│   │   │   ├── network.rs         # Simulated network
│   │   │   ├── clock.rs           # Virtual clock
│   │   │   ├── fault.rs           # Fault injection engine
│   │   │   └── explore.rs         # Seed exploration / coverage
│   │   └── Cargo.toml             # deps: chaoscontrol-vmm
│   │
│   ├── chaoscontrol-guest/        # Minimal guest image builder
│   │   ├── kernel/                # Kernel config
│   │   ├── initrd/                # Minimal initrd with your service
│   │   └── build.rs               # Builds the guest image
│   │
│   └── chaoscontrol-cli/          # CLI for running tests
│       └── src/main.rs
│
├── tests/
│   └── integration/
│       ├── kv_store.rs            # Test a KV store cluster
│       └── consensus.rs           # Test a consensus protocol
│
└── docs/
    └── deterministic-vmm-design.md  # This file
```

## Dependencies (Cargo.toml)

```toml
[dependencies]
kvm-ioctls = "0.19"
kvm-bindings = { version = "0.10", features = ["fam-wrappers"] }
vm-memory = { version = "0.16", features = ["backend-mmap"] }
linux-loader = "0.13"
vmm-sys-util = "0.12"
vm-superio = "0.8"

# Determinism
rand_chacha = "0.3"          # Seeded PRNG

# Snapshot serialization
serde = { version = "1", features = ["derive"] }
bincode = "1"

# Guest image building
# (uses nix or docker to build minimal kernel + initrd)
```

## First Milestone: Boot & Snapshot

Week 1-2: Boot a minimal Linux kernel in a single-vCPU KVM VM using
rust-vmm crates. Verify serial console output. Take a snapshot,
restore it, verify execution continues.

Week 3-4: Add determinism controls (CPUID filter, TSC pin, MSR trap,
seeded virtio-rng). Verify: boot same kernel+initrd with same seed
100 times → identical dmesg output.

Week 5-6: Add deterministic virtio-net. Connect two VMs. Send
packets between them. Verify deterministic delivery.

Week 7-8: Build the simulation controller. Run a simple service
(echo server) across 3 VMs. Inject network partition. Check
correctness.
