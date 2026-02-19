# ChaosControl Glossary

Quick reference for every term, acronym, and internal concept used in
the ChaosControl codebase.  Grouped by domain; alphabetical within each
section.

---

## Hardware & CPU Virtualization

| Term | Definition |
|------|-----------|
| **APIC** | Advanced Programmable Interrupt Controller — per-core local interrupt controller on x86. See also LAPIC, IOAPIC. |
| **CPUID** | x86 instruction that returns processor identification and feature flags. ChaosControl filters CPUID to hide non-deterministic features (RDRAND, RDTSCP, TSC-Deadline). |
| **CR** | Control Register — x86 registers (CR0–CR4) controlling processor modes (paging, protected mode, etc.). KVM traps writes to CRs. |
| **E820** | BIOS memory map passed to the kernel at boot. Describes which physical address ranges are usable RAM, reserved, or ACPI. ChaosControl's coverage bitmap lives in a reserved E820 gap at `0xE0000`. |
| **EPT** | Extended Page Tables — Intel's hardware-assisted nested paging. Maps guest physical addresses (GPA) to host physical addresses (HPA) without shadow page tables. |
| **GPA** | Guest Physical Address — a physical address as seen by the guest VM. Translated to HPA by EPT/NPT. |
| **GVA** | Guest Virtual Address — a virtual address inside the guest, translated by the guest's own page tables. |
| **HLT** | x86 halt instruction — puts the CPU into a low-power state until the next interrupt. ChaosControl fast-forwards virtual time on HLT. |
| **HPA** | Host Physical Address — the real physical address on the host machine. |
| **HVA** | Host Virtual Address — a virtual address in the host (VMM) process, corresponding to a GPA via mmap'd guest memory. |
| **IOAPIC** | I/O APIC — system-level interrupt controller that routes device interrupts to per-core LAPICs. |
| **IRQ** | Interrupt Request — a hardware signal from a device requesting CPU attention. Numbered 0–15 (legacy PIC) or 0–23+ (IOAPIC). |
| **KVM** | Kernel-based Virtual Machine — Linux kernel module providing hardware-assisted virtualization. ChaosControl's execution engine. |
| **LAPIC** | Local APIC — per-core interrupt controller. Handles inter-processor interrupts (IPIs), timer, and local interrupt delivery. |
| **MMIO** | Memory-Mapped I/O — device registers accessed via memory read/write at specific physical addresses (not I/O ports). Virtio MMIO devices use this. |
| **MSR** | Model-Specific Register — x86 registers for configuration and status (e.g., `IA32_TSC`, `IA32_TSC_AUX`). Accessed via `rdmsr`/`wrmsr`. |
| **NMI** | Non-Maskable Interrupt — high-priority interrupt that cannot be disabled. Used for watchdogs, critical errors. |
| **PIC** | Programmable Interrupt Controller — legacy 8259A dual-PIC (master + slave). Handles IRQ 0–15. |
| **PIO** | Port I/O — x86 I/O space accessed via `in`/`out` instructions (separate from memory address space). Serial, PIT, and SDK use PIO. |
| **RDRAND** | x86 instruction returning hardware random numbers. **Disabled** in ChaosControl via CPUID filtering — non-deterministic. |
| **RDSEED** | x86 instruction returning entropy samples from the hardware RNG. **Disabled** — same reason as RDRAND. |
| **RDTSC** | Read Time-Stamp Counter — x86 instruction returning the TSC value. ChaosControl traps/controls this via `IA32_TSC` MSR writes. |
| **RDTSCP** | Read TSC and Processor ID — like RDTSC but also reads `IA32_TSC_AUX`. **Disabled** via CPUID filtering. |
| **SMI** | System Management Interrupt — enters SMM (System Management Mode). Not used by ChaosControl. |
| **SMP** | Symmetric Multi-Processing — multiple CPU cores sharing memory. ChaosControl currently uses single-vCPU VMs. |
| **SVM** | Secure Virtual Machine — AMD's hardware virtualization extension (equivalent to Intel VMX). |
| **TSC** | Time Stamp Counter — 64-bit counter incremented at a fixed rate. ChaosControl pins the guest TSC to its virtual clock for determinism. |
| **VCPU** / **vCPU** | Virtual CPU — a virtualized processor core inside a VM, managed by KVM. Each ChaosControl VM has one vCPU. |
| **VMCS** | Virtual Machine Control Structure — Intel data structure controlling VM entry/exit behavior. Managed by KVM. |
| **VMX** | Virtual Machine Extensions — Intel's hardware virtualization instruction set. |

---

## Time & Clocks

| Term | Definition |
|------|-----------|
| **kvmclock** | Paravirtual clock source where the guest reads time from a shared page written by the host. ChaosControl hides the hypervisor to prevent kvm-clock registration. |
| **PIT** | Programmable Interval Timer — Intel 8254 chip. Channel 0 drives IRQ 0 (system timer). Linux uses it for early TSC calibration. ChaosControl uses a hybrid PIT: KVM PIT for I/O routing + DeterministicPit for controlled time. |
| **RTC** | Real-Time Clock — battery-backed clock (MC146818). Provides wall-clock time and periodic interrupts. |
| **tick** | ChaosControl's discrete time unit. 1 tick ≈ 1 ms. The global simulation clock advances in ticks. All scheduling, network delivery, and fault timing use ticks. |
| **vTSC** / **Virtual TSC** | ChaosControl's deterministic TSC. Advances by a fixed amount per VM exit. Written to `IA32_TSC` before every `vcpu.run()` to override KVM's real-time TSC. |

---

## Devices & I/O

| Term | Definition |
|------|-----------|
| **bzImage** | Compressed Linux kernel image (boot sector + setup + compressed vmlinux). ChaosControl does NOT use bzImage — requires raw ELF vmlinux. |
| **DMA** | Direct Memory Access — hardware accessing memory without CPU involvement. Virtio uses DMA-like shared-memory queues. |
| **ELF** | Executable and Linkable Format — binary format. ChaosControl loads the kernel as an ELF file (vmlinux). |
| **initrd** / **initramfs** | Initial RAM disk/filesystem — compressed archive loaded into RAM at boot. Contains the guest program (PID 1). |
| **musl** | Minimal C standard library for static linking. Guest programs use `x86_64-unknown-linux-musl` target for fully static binaries. |
| **virtio** | Standardized paravirtual I/O framework. ChaosControl implements virtio-blk, virtio-net, and virtio-rng over MMIO transport. |
| **virtio-blk** | Virtio block device — provides deterministic disk I/O to the guest. Supports fault injection (read errors, write errors, torn writes, corruption). |
| **virtio-net** | Virtio network device — connects the guest to the NetworkFabric. Packets sent via virtio queues are routed through the fabric's fault pipeline. |
| **virtio-rng** | Virtio entropy device — provides deterministic random bytes from `DeterministicEntropy` (ChaCha20-based PRNG). |
| **vmlinux** | Uncompressed Linux kernel ELF binary. This is what ChaosControl loads (not bzImage). Built via `nix build .#kernel-dev` → `result-dev/vmlinux`. |

---

## ChaosControl Architecture

| Term | Definition |
|------|-----------|
| **DeterministicEntropy** | Seeded ChaCha20 PRNG replacing hardware RNG. Same seed → same random output across runs. Supports snapshot/restore. (`devices/entropy.rs`) |
| **DeterministicPit** | Controlled PIT implementation that returns virtual time instead of host time. Mirrors KVM PIT state and syncs `count_load_time` to virtual TSC. (`devices/pit.rs`) |
| **DeterministicVm** | Single-VM wrapper. Handles kernel loading, CPUID filtering, TSC pinning, device setup, and deterministic exit handling. (`vm.rs`) |
| **HypercallPage** | 4096-byte shared memory page at GPA `0x000F_E000`. Guest SDK writes commands; VMM reads, processes, writes responses. (`chaoscontrol-protocol`) |
| **NetworkFabric** | Simulated network connecting all VMs. Routes packets through a 7-stage fault pipeline: partition → loss → bandwidth → corruption → latency+jitter → reorder → duplication. (`controller.rs`) |
| **NetworkMessage** | A packet in flight within the NetworkFabric. Contains `from`, `to`, `data`, and `deliver_at_tick`. |
| **NetworkStats** | Cumulative packet-level counters (sent, delivered, dropped, corrupted, duplicated, jittered, etc.) for observability. |
| **SimulationController** | Top-level orchestrator for multi-VM simulations. Manages VMs, NetworkFabric, FaultEngine, scheduling rounds, and snapshot/restore. (`controller.rs`) |
| **VmSlot** | Per-VM state within SimulationController: the VM itself, status, message inbox, disk fault flags, TSC skew, memory limit, initial snapshot. |
| **VmStatus** | VM lifecycle state: `Running`, `Paused`, `Resuming`, `Killed`, `Halted`. Governs scheduling in `step_round()`. |

---

## Fault Injection

| Term | Definition |
|------|-----------|
| **ClockJump** | Fault that adds a one-time offset to a VM's virtual TSC. Simulates sudden clock discontinuity. |
| **ClockSkew** | Fault that applies a persistent offset to a VM's virtual TSC. Simulates clock drift. |
| **DiskCorruption** | Fault that flips random bytes in disk read responses. |
| **DiskFull** | Fault that makes all disk writes fail with "no space". |
| **DiskReadError** | Fault that makes disk reads return I/O errors. |
| **DiskTornWrite** | Fault that partially applies disk writes (simulates power loss mid-write). |
| **DiskWriteError** | Fault that makes disk writes return I/O errors. |
| **Fault** | Enum of all injectable fault types. Each variant targets a specific VM and carries parameters. (`chaoscontrol-fault`) |
| **FaultEngine** | Drives fault dispatch: holds the schedule, generates random faults, tracks timing, and interfaces with the PropertyOracle. (`engine.rs`) |
| **FaultSchedule** | Time-ordered list of `(time_ns, Fault)` pairs. Faults fire when the simulation clock passes their scheduled time. Built via `FaultScheduleBuilder`. |
| **MemoryPressure** | Fault that sets a memory limit on a VM. Stored in `VmSlot.memory_limit_bytes`. |
| **NetworkBandwidth** | Fault that caps a VM's throughput. Implemented as serialization delay with queuing (`next_free_tick` model). |
| **NetworkHeal** | Meta-fault that clears all network impairments: partitions, loss, corruption, reorder, jitter, bandwidth, duplication. Base latency is preserved. |
| **NetworkJitter** | Fault that adds random delay (0 to `jitter_ticks`) on top of base latency per packet. |
| **NetworkLatency** | Fault that sets base per-VM latency in ticks. |
| **NetworkPartition** | Fault that isolates groups of VMs so packets between groups are dropped. |
| **PacketCorruption** | Fault that flips a random byte in packet data. Rate in PPM. |
| **PacketDuplicate** | Fault that creates a copy of a packet with 0–2 ticks extra offset. Rate in PPM. |
| **PacketLoss** | Fault that drops packets. Rate in PPM. |
| **PacketReorder** | Fault that adds random extra delay within a window, causing out-of-order delivery. |
| **PPM** | Parts Per Million — unit for probabilistic fault rates. 1,000,000 PPM = 100%. Example: 300,000 PPM = 30% chance. |
| **ProcessKill** | Fault that transitions a VM to `Killed` status (no longer scheduled). |
| **ProcessPause** | Fault that transitions a VM to `Paused` status. Can auto-resume. |
| **ProcessRestart** | Fault that restores a VM from its initial snapshot (fresh boot state). |
| **setup_complete** | SDK lifecycle call. Faults are gated — none fire until the guest calls `setup_complete()`. Use `force_setup_complete()` in tests when the guest doesn't use the SDK. |

---

## Exploration & Coverage

| Term | Definition |
|------|-----------|
| **AFL** | American Fuzzy Lop — coverage-guided fuzzer. ChaosControl uses AFL-style edge coverage bitmaps. |
| **bitmap** | 64 KB array at GPA `0xE0000`. Each byte is a saturating counter for an edge (source→target basic block transition). |
| **branch** | A single simulation run from a snapshot point. The explorer forks multiple branches from each frontier entry to explore different fault schedules. |
| **BranchResult** | Result of one branch execution: coverage delta, assertion outcomes, ticks elapsed. |
| **BugReport** | Record of a detected assertion failure: bug ID, assertion ID, location, schedule, snapshot, tick. |
| **corpus** | Collection of interesting inputs (fault schedules + snapshots) that found new coverage. Grows monotonically during exploration. |
| **coverage** / **edge coverage** | Count of unique control-flow edges (basic block transitions) observed during execution. More edges = more code paths explored. |
| **CoverageCollector** | Reads the coverage bitmap from guest memory and tracks which edges have been seen across all runs. |
| **explorer** | The top-level exploration loop: pick frontier entry → fork branches → collect coverage → update corpus. (`chaoscontrol-explore`) |
| **ExplorationReport** | Final summary: rounds, branches, edges, bugs, corpus size, coverage stats, network stats. |
| **frontier** | Priority queue of (snapshot, schedule) pairs to explore next. Entries that found new coverage get priority. |
| **guard** | SanCov trace-pc-guard — compiler-inserted callback at each basic block edge. ChaosControl's SDK implements the guard to update the coverage bitmap. |
| **kcov** | Kernel coverage — Linux kernel's built-in coverage tracing. ChaosControl uses SanCov-style instrumentation in guest user-space instead. |
| **mutator** | Generates new fault schedules by mutating existing ones: add/remove/shift faults, change parameters, splice two schedules. |
| **round** | One iteration of the exploration loop: pick a frontier entry, run N branches from it, collect results. |
| **SanCov** | Sanitizer Coverage — Clang/GCC instrumentation that inserts callbacks at control-flow edges. ChaosControl's SDK hooks `__sanitizer_cov_trace_pc_guard`. |
| **seed** | (1) Master RNG seed for a simulation — determines all random decisions (fault generation, network RNG, per-VM entropy). (2) An initial corpus entry (fault schedule). |

---

## Replay & Debugging

| Term | Definition |
|------|-----------|
| **checkpoint** | Periodic snapshot during recording. Contains VM snapshots, network state, fault engine state, and events since last checkpoint. Enables fast seeking during replay. |
| **debugger** | Time-travel debugger. Navigates recorded events: step forward/backward, seek to tick, filter by event type, inspect state at any point. (`debugger.rs`) |
| **EventFilter** | Predicate for selecting events during replay: `AnyFault`, `AnyAssertion`, `FailedAssertion`, `AnyBug`, `VmStatusChange`, `SerialOutput`. |
| **RecordedEvent** | Tagged union of events captured during execution: `FaultFired`, `AssertionHit`, `VmStatusChange`, `SerialOutput`, `BugDetected`. |
| **Recorder** | Writes events and periodic checkpoints during simulation execution. (`recording.rs`) |
| **Recording** | Complete execution trace: config, events, checkpoints. Serializable to JSON for storage and sharing. |
| **replay** | Re-execute a recorded simulation from its fault schedule. Produces identical execution (deterministic). |
| **triage** | Automated bug analysis: clusters bugs by assertion, shows timeline of events, identifies minimal reproduction schedule. (`triage.rs`) |

---

## SDK & Protocol

| Term | Definition |
|------|-----------|
| **always** | SDK assertion macro: property must hold on every call across every run. Tracked by PropertyOracle. |
| **chaoscontrol-protocol** | Wire-format crate (`no_std`, zero deps). Defines HypercallPage layout, command IDs, and payload encoding. |
| **chaoscontrol-sdk** | Guest SDK crate. Provides `assert::{always, sometimes, reachable, unreachable}`, `lifecycle::{setup_complete, send_event}`, `random::{get_random, random_choice}`, and coverage instrumentation. |
| **FNV-1a** | Fowler–Noll–Vo hash — fast non-cryptographic hash. Used to compute assertion IDs from source location strings. |
| **get_random** | SDK call returning a deterministic pseudo-random u64 from the VMM's entropy source. |
| **hypercall** | Guest→VMM communication via shared memory page + port I/O trigger. Not a real x86 hypercall — uses `outb(0x0510)`. |
| **location_id** | Compile-time FNV-1a hash of a source location string (e.g., `"src/main.rs:42"`). Used as assertion identity. |
| **PropertyOracle** | Tracks assertion outcomes across multiple runs. Reports violations (an `always` that was `false`), unreached assertions, and coverage. (`oracle.rs`) |
| **random_choice** | SDK call returning a deterministic random index in `[0, n)`. Used for controlled non-determinism in guest programs. |
| **reachable** | SDK assertion: this code path must be reached at least once across all runs. |
| **send_event** | SDK lifecycle call: sends a named event to the VMM for recording. |
| **sometimes** | SDK assertion: property must hold on at least one call across all runs. |
| **unreachable** | SDK assertion: this code path must never be reached. If it is, that's a bug. |

---

## Crypto & RNG

| Term | Definition |
|------|-----------|
| **ChaCha20** | Stream cipher used as ChaosControl's deterministic PRNG (via `rand_chacha::ChaCha20Rng`). Seeded from the master seed with domain-specific key derivation. |
| **domain separator** | Constant mixed into the seed to derive independent RNG streams. NetworkFabric uses `seed + 0x4E45_5446_4142` ("NETFAB"), fault engine uses raw seed, per-VM uses `seed + vm_index`. |
| **PRNG** | Pseudo-Random Number Generator — deterministic algorithm producing random-looking output from a seed. |
| **RNG** | Random Number Generator — in ChaosControl, always refers to a seeded PRNG (never hardware randomness). |
| **SeedableRng** | Rust trait from the `rand` crate. Allows creating an RNG from a fixed seed for reproducibility. |

---

## Build & Tooling

| Term | Definition |
|------|-----------|
| **CARGO_TARGET_DIR** | Environment variable overriding Cargo's build output directory. Set to `~/.cargo-target` in the Nix devshell. |
| **clippy** | Rust linter. Run via `cargo clippy`. Pre-existing warnings in guest/SDK crates are known (manual_c_str_literals, assertions_on_constants). |
| **devshell** | Nix development environment. All cargo commands must run inside `nix develop --command bash -c "..."`. |
| **flake.nix** | Nix flake defining the project's build inputs, devshell, and packages (kernel, initrd, binaries). |
| **libbpf-rs** | Rust bindings for libbpf. Used by chaoscontrol-trace for eBPF tracepoint programs. |
| **Nix** | Functional package manager. ChaosControl uses Nix flakes for reproducible builds and development environments. |
| **rust-vmm** | Collection of Rust crates for building VMMs: `kvm-ioctls`, `kvm-bindings`, `vm-memory`, `linux-loader`, `vm-superio`. |
| **tarpaulin** | Rust code coverage tool (`cargo-tarpaulin`). Used to verify test coverage of guest libraries (e.g., 100% line coverage on Raft). |

---

## Crates

| Crate | Purpose |
|-------|---------|
| **chaoscontrol-vmm** | Core VMM: DeterministicVm, SimulationController, NetworkFabric, devices, integration tests. |
| **chaoscontrol-fault** | Fault types, FaultEngine, FaultSchedule, PropertyOracle. Host-side only. |
| **chaoscontrol-protocol** | Wire format between guest SDK and VMM. `no_std`, zero dependencies. |
| **chaoscontrol-sdk** | Guest-side SDK. Assertions, lifecycle, random, coverage instrumentation. `no_std` + optional `std`. |
| **chaoscontrol-explore** | Coverage-guided exploration: Explorer, Frontier, Corpus, Mutator, CoverageCollector, report formatting. |
| **chaoscontrol-replay** | Recording, replay, checkpoint, time-travel debugger, triage, serialization. |
| **chaoscontrol-trace** | eBPF tracing harness for KVM tracepoints. Used for debugging non-determinism. |

---

## VcpuExit Reasons

| Exit | Meaning |
|------|---------|
| **Hlt** | Guest executed HLT. ChaosControl fast-forwards virtual TSC to next timer interrupt. |
| **InternalError** | KVM internal error (e.g., unhandled emulation). Fatal. |
| **Intr** | Host signal interrupted `vcpu.run()`. Retry without advancing time — NOT a guest event. |
| **IoIn** | Guest performed `in` (port read). Dispatched to PIT, serial, SDK, or coverage port handler. |
| **IoOut** | Guest performed `out` (port write). Dispatched to PIT, serial, SDK, or coverage port handler. |
| **MmioRead** | Guest read from an MMIO address. Dispatched to virtio MMIO device. |
| **MmioWrite** | Guest wrote to an MMIO address. Dispatched to virtio MMIO device. |
| **Shutdown** | Guest triple-faulted or called shutdown. VM transitions to Halted. |

---

## Network Fabric Pipeline Stages

| Stage | What Happens |
|-------|-------------|
| 1. **Partition** | Drop if sender and receiver are in different partition sides. |
| 2. **Loss** | Drop with probability `max(sender_ppm, receiver_ppm)`. |
| 3. **Bandwidth** | Add serialization delay: `bits × 1000 / effective_bps`. Queuing via `next_free_tick`. |
| 4. **Corruption** | Flip a random byte with probability `max(sender_ppm, receiver_ppm)`. |
| 5. **Latency + Jitter** | Base delay `max(sender, receiver)` + random `[0, jitter_max]` ticks. |
| 6. **Reorder** | Add random extra delay within `reorder_window` ticks. |
| 7. **Duplication** | Clone packet with probability; duplicate arrives 0–2 ticks later. |

---

## Key Addresses & Ports

| Address / Port | Purpose |
|---------------|---------|
| `0x000F_E000` (GPA) | HypercallPage — SDK ↔ VMM shared memory. |
| `0x0_E0000` (GPA) | Coverage bitmap — 64 KB AFL-style edge counters. |
| `0x0510` (port) | SDK trigger port — `outb` triggers hypercall processing. |
| `0x0511` (port) | Coverage port — `outb` signals coverage bitmap is active. |
| `0x40`–`0x43` (port) | PIT channels 0–2 and control register. |
| `0x61` (port) | PIT channel 2 gate / speaker control (used during TSC calibration). |
| `0x3F8` (port) | COM1 serial — guest console output. |
| `0xD000_0000` (GPA) | Virtio MMIO device 0 (block), IRQ 5. |
| `0xD000_1000` (GPA) | Virtio MMIO device 1 (network), IRQ 6. |
| `0xD000_2000` (GPA) | Virtio MMIO device 2 (entropy/RNG), IRQ 7. |
