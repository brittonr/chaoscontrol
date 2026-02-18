# Deterministic Simulation Testing Hypervisor

## Building an Antithesis-like System on Hyperlight

### Executive Summary

This document designs a **deterministic simulation testing hypervisor** inspired
by [Antithesis](https://antithesis.com), built on top of Microsoft's
[Hyperlight](https://github.com/hyperlight-dev/hyperlight) micro-VM framework.
The goal: run arbitrary software inside lightweight VMs where **every source of
non-determinism is controlled**, enabling exact replay, systematic fault
injection, and automated bug discovery.

---

## 1. Background: How Antithesis Works

Antithesis runs unmodified software inside a **fully deterministic virtual
machine**. Their key insights:

1. **Determinism at the hypervisor level** — Instead of modifying the software
   under test, control non-determinism in the VM layer. All software running
   inside becomes automatically deterministic.

2. **Sources of non-determinism to control:**
   - Thread scheduling order
   - Time (wall clock, monotonic clock, `rdtsc`)
   - Random number generators (entropy sources)
   - Network I/O ordering and timing
   - Disk I/O ordering
   - Signal delivery order
   - Memory allocation addresses (ASLR)

3. **Snapshot + Fork** — Take VM snapshots at any point, fork execution to
   explore different scheduling decisions, replay from any snapshot with the
   same seed to get identical behavior.

4. **Intelligent fault injection** — Systematically inject faults (network
   partitions, disk errors, process crashes, OOM) at every interesting point
   to find bugs that only manifest under failure conditions.

5. **Property-based testing at scale** — Run thousands of divergent executions
   from the same starting point, varying only the scheduling seed.

---

## 2. Why Hyperlight is a Good Foundation

Hyperlight provides several properties that align well with this goal:

| Hyperlight Feature | Relevance to Deterministic Simulation |
|---|---|
| **Micro-VM architecture** | Minimal guest state = fast snapshots, small state to control |
| **No guest OS** | No kernel scheduling, no OS-level non-determinism to tame |
| **Host-guest boundary via `outb` ports** | Natural interception point for ALL guest→host communication |
| **Host functions** | Guest calls host functions via `OutBAction::CallFunction` — we intercept these |
| **Snapshot/restore built-in** | `Snapshot` struct already captures full memory + register state + page tables |
| **Single vCPU per sandbox** | Eliminates hardware-level thread scheduling non-determinism |
| **KVM/MSHV/WHP backends** | VM exits give us control points (I/O, MMIO, HLT) |
| **Rust implementation** | Memory safety, good ecosystem for systems programming |
| **Fast startup** | Sub-millisecond sandbox creation enables rapid test iteration |

### Key Architectural Advantage

Hyperlight guests have **no OS kernel**. They run as bare-metal programs with a
minimal runtime (`hyperlight_guest_bin`). This means:

- **No kernel scheduler** — The guest runs single-threaded on one vCPU. Thread
  scheduling is 100% our problem to solve (via cooperative multitasking in the
  guest runtime).
- **No syscalls** — All I/O goes through `outb` port instructions which cause
  VM exits that we handle. We already intercept these in `handle_outb()`.
- **No ASLR** — Guest memory layout is fixed by `SandboxMemoryLayout`.
- **No `/dev/urandom`** — Entropy must come through host functions we control.

---

## 3. Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                     Deterministic Test Harness                      │
│  ┌───────────────┐  ┌──────────────┐  ┌─────────────────────────┐  │
│  │  Test Oracle   │  │ Fault Inject │  │  Execution Explorer     │  │
│  │  (Properties)  │  │   Engine     │  │  (Seed-based forking)   │  │
│  └───────┬───────┘  └──────┬───────┘  └────────────┬────────────┘  │
│          │                 │                        │               │
│  ┌───────▼─────────────────▼────────────────────────▼────────────┐  │
│  │              Deterministic Simulation Controller               │  │
│  │  ┌─────────────┐ ┌──────────────┐ ┌────────────────────────┐  │  │
│  │  │ Det. Clock  │ │ Det. Sched.  │ │ Det. I/O Mediator      │  │  │
│  │  │ (virtual    │ │ (cooperative │ │ (network, disk,        │  │  │
│  │  │  time)      │ │  + preempt)  │ │  entropy)              │  │  │
│  │  └─────────────┘ └──────────────┘ └────────────────────────┘  │  │
│  └───────────────────────────┬───────────────────────────────────┘  │
│                              │                                      │
│  ┌───────────────────────────▼───────────────────────────────────┐  │
│  │              Hyperlight Sandbox Layer (modified)               │  │
│  │  ┌──────────┐  ┌───────────────┐  ┌────────────────────────┐  │  │
│  │  │ Snapshot  │  │  VM Exit      │  │  Memory Manager        │  │  │
│  │  │ Manager   │  │  Interceptor  │  │  (det. allocation)     │  │  │
│  │  └──────────┘  └───────────────┘  └────────────────────────┘  │  │
│  └───────────────────────────┬───────────────────────────────────┘  │
│                              │                                      │
│  ┌───────────────────────────▼───────────────────────────────────┐  │
│  │                    KVM / Hardware Virtualization               │  │
│  │            (single vCPU, controlled VM exits)                  │  │
│  └───────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘

   ┌─────────────────────────────────────────────────────────────────┐
   │                    Guest Sandboxes (N instances)                 │
   │  ┌──────────────────┐  ┌──────────────────┐                     │
   │  │  Service A        │  │  Service B        │                    │
   │  │  (your code +     │  │  (your code +     │  ← Unmodified*    │
   │  │   det. runtime)   │  │   det. runtime)   │    application    │
   │  │                   │  │                   │    code            │
   │  │  ┌─────────────┐  │  │  ┌─────────────┐  │                   │
   │  │  │ Det. Guest  │  │  │  │ Det. Guest  │  │  ← Deterministic │
   │  │  │ Runtime     │  │  │  │ Runtime     │  │    guest shim    │
   │  │  └─────────────┘  │  │  └─────────────┘  │                   │
   │  └──────────────────┘  └──────────────────┘                     │
   └─────────────────────────────────────────────────────────────────┘
```

---

## 4. Controlling Every Source of Non-Determinism

### 4.1 Time — Virtual Deterministic Clock

**Problem:** `rdtsc`/`rdtscp` instructions, clock_gettime, and any
time-dependent logic.

**Solution:** Intercept all time sources and replace with a virtual clock.

```rust
/// Virtual clock that advances deterministically.
/// Time only moves forward when the controller explicitly ticks it.
pub struct DeterministicClock {
    /// Current virtual time in nanoseconds
    virtual_nanos: u64,
    /// Deterministic seed for jitter (if simulating realistic timing)
    rng: DeterministicRng,
}

impl DeterministicClock {
    /// Advance time by a deterministic amount
    pub fn tick(&mut self, nanos: u64) {
        self.virtual_nanos += nanos;
    }

    /// Get current virtual time
    pub fn now(&self) -> u64 {
        self.virtual_nanos
    }

    /// Advance time to the next scheduled event
    pub fn advance_to_next_event(&mut self, event_queue: &EventQueue) {
        if let Some(next) = event_queue.peek() {
            self.virtual_nanos = next.scheduled_time;
        }
    }
}
```

**Interception points in Hyperlight:**

1. **`rdtsc` / `rdtscp` interception** — Configure KVM to trap these
   instructions via `KVM_CAP_TSC_CONTROL` or by setting the TSC offset. On
   KVM, use `KVM_SET_TSC_KHZ` to pin the TSC frequency, and
   `KVM_CAP_ADJUST_CLOCK` to set a fixed TSC value. Alternatively, enable
   RDTSC exiting via the VMCS/VMCB:

   ```rust
   // In KvmVm::new(), configure CPUID to disable rdtsc or
   // set up VMX procctl to intercept RDTSC
   // KVM: Use KVM_SET_CLOCK / KVM_GET_CLOCK
   // Or: trap RDTSC via #UD by clearing CR4.TSD and using CPUID filtering

   // Better approach: Use KVM_CAP_TSC_CONTROL
   fn setup_deterministic_tsc(vcpu_fd: &VcpuFd, initial_tsc: u64) {
       // Pin TSC to a known value
       vcpu_fd.set_tsc_khz(1_000_000); // Fixed 1GHz
       // Set initial TSC value
       let mut msrs = kvm_bindings::Msrs::from_entries(&[
           kvm_bindings::kvm_msr_entry {
               index: 0x10, // IA32_TIME_STAMP_COUNTER
               data: initial_tsc,
               ..Default::default()
           }
       ]).unwrap();
       vcpu_fd.set_msrs(&msrs).unwrap();
   }
   ```

2. **Host function interception** — Any guest function that asks for time
   (e.g., a `get_time` host function) returns virtual clock values:

   ```rust
   fn register_deterministic_time_functions(
       sandbox: &mut UninitializedSandbox,
       clock: Arc<Mutex<DeterministicClock>>,
   ) {
       sandbox.register_host_function("get_time_ns", move || {
           let clock = clock.lock().unwrap();
           Ok(clock.now())
       });
   }
   ```

### 4.2 Thread Scheduling — Cooperative Deterministic Scheduler

**Problem:** Thread interleaving is a massive source of non-determinism and
concurrency bugs.

**Solution:** Since Hyperlight runs single-vCPU sandboxes, we implement a
**cooperative user-space scheduler** in the guest runtime, controlled by the
host.

```rust
/// A deterministic thread scheduler that runs in the host,
/// controlling which guest "thread" runs next.
pub struct DeterministicScheduler {
    /// Seed-based RNG for scheduling decisions
    rng: DeterministicRng,
    /// Ready queue of runnable threads
    ready: VecDeque<ThreadId>,
    /// Threads waiting on I/O, locks, timers
    blocked: HashMap<ThreadId, BlockReason>,
    /// Current running thread
    current: Option<ThreadId>,
    /// Number of instructions before forced preemption
    quantum: u64,
    /// Instruction counter for current thread
    instructions_executed: u64,
}

impl DeterministicScheduler {
    /// Pick the next thread to run. Deterministic given the same seed.
    pub fn schedule_next(&mut self) -> Option<ThreadId> {
        if self.ready.is_empty() {
            return None;
        }
        // Use deterministic RNG to pick from ready queue
        // This allows exploring different interleavings with different seeds
        let idx = self.rng.next_u64() as usize % self.ready.len();
        let thread = self.ready.remove(idx).unwrap();
        self.current = Some(thread);
        self.instructions_executed = 0;
        Some(thread)
    }

    /// Called on every yield point or after quantum expires
    pub fn yield_current(&mut self) {
        if let Some(current) = self.current.take() {
            self.ready.push_back(current);
        }
    }
}
```

**Implementation strategy:**

The guest runtime provides a threading API where:
- `thread_create()` calls a host function → host scheduler tracks it
- `mutex_lock()` / `cond_wait()` call host functions → host blocks/unblocks
- `yield()` calls a host function → host picks next thread
- The host uses KVM's **single-stepping** or **instruction counting** (via
  performance counters) to enforce preemption after a deterministic quantum

```rust
/// Guest-side threading API (in hyperlight_guest)
#[no_mangle]
pub extern "C" fn det_thread_create(entry: fn()) -> ThreadId {
    // Calls host function to register thread
    call_host_function("sched_thread_create", &[entry as u64])
}

#[no_mangle]
pub extern "C" fn det_mutex_lock(mutex: &Mutex) {
    call_host_function("sched_mutex_lock", &[mutex.id()])
}

#[no_mangle]
pub extern "C" fn det_yield() {
    call_host_function("sched_yield", &[])
}
```

**Preemption via hardware performance counters:**

KVM supports `KVM_SET_GUEST_DEBUG` with hardware breakpoints and single-step
mode. More importantly, we can use **PMU-based instruction counting** to inject
a VM exit after N instructions:

```rust
/// Set up deterministic preemption using performance counters
fn setup_instruction_counting(vcpu_fd: &VcpuFd, quantum: u64) {
    // Use KVM's PMU virtualization to count retired instructions
    // and trigger a VM exit after `quantum` instructions
    // This uses FIXED_CTR0 (instructions retired) with overflow interrupt
    //
    // Alternative: Use KVM_GUESTDBG_SINGLESTEP for fine-grained control
    // (much slower but perfectly deterministic)
}
```

### 4.3 Entropy — Seeded Deterministic RNG

**Problem:** Random number generators, `/dev/urandom`, RDRAND instruction.

**Solution:** Hyperlight already passes a `seed` parameter during guest
initialization (see `HyperlightVm::initialise()`):

```rust
// From hyperlight_vm.rs - already exists!
pub(crate) fn initialise(
    &mut self,
    peb_addr: RawPtr,
    seed: u64,        // ← This is our deterministic seed!
    page_size: u32,
    // ...
)
```

**Additional steps:**

1. **Intercept RDRAND/RDSEED** — Configure CPUID to report these as
   unavailable, or trap them via VM exits:

   ```rust
   fn disable_hardware_rng(cpuid: &mut CpuId) {
       for entry in cpuid.as_mut_slice() {
           if entry.function == 1 {
               entry.ecx &= !(1 << 30); // Clear RDRAND bit
           }
           if entry.function == 7 && entry.index == 0 {
               entry.ebx &= !(1 << 18); // Clear RDSEED bit
           }
       }
   }
   ```

2. **Provide deterministic entropy host function:**

   ```rust
   fn register_entropy_functions(
       sandbox: &mut UninitializedSandbox,
       rng: Arc<Mutex<DeterministicRng>>,
   ) {
       sandbox.register_host_function("get_random_bytes", move |len: u32| {
           let mut rng = rng.lock().unwrap();
           let mut buf = vec![0u8; len as usize];
           rng.fill_bytes(&mut buf);
           Ok(buf)
       });
   }
   ```

### 4.4 Network I/O — Simulated Deterministic Network

**Problem:** Network packet ordering, timing, and failures are non-deterministic.

**Solution:** Replace real networking with a **simulated network** controlled by
the deterministic controller.

```rust
/// Simulated network connecting multiple guest sandboxes
pub struct DeterministicNetwork {
    /// In-flight packets, ordered by delivery time
    packets: BTreeMap<VirtualTime, Packet>,
    /// Per-sandbox send/receive queues
    endpoints: HashMap<SandboxId, NetworkEndpoint>,
    /// RNG for simulating jitter, reordering, loss
    rng: DeterministicRng,
    /// Fault injection rules
    faults: Vec<NetworkFault>,
}

pub struct Packet {
    src: SandboxId,
    dst: SandboxId,
    data: Vec<u8>,
    scheduled_delivery: VirtualTime,
}

pub enum NetworkFault {
    /// Drop packets between these endpoints
    Partition { a: SandboxId, b: SandboxId, duration: VirtualTime },
    /// Delay packets by extra amount
    Latency { target: SandboxId, extra_ns: u64 },
    /// Randomly drop N% of packets
    PacketLoss { target: SandboxId, rate: f64 },
    /// Duplicate packets
    Duplicate { target: SandboxId, rate: f64 },
    /// Reorder packets within a window
    Reorder { target: SandboxId, window_ns: u64 },
}
```

**Guest-side networking API:**

```rust
// In deterministic guest runtime
pub fn net_send(dst: SandboxId, data: &[u8]) -> Result<()> {
    call_host_function("net_send", &[dst, data])
}

pub fn net_recv(timeout_ns: u64) -> Result<Option<(SandboxId, Vec<u8>)>> {
    call_host_function("net_recv", &[timeout_ns])
}

pub fn net_listen(port: u16) -> Result<Listener> {
    call_host_function("net_listen", &[port])
}
```

### 4.5 Disk I/O — Virtual Deterministic Filesystem

```rust
/// Simulated filesystem with deterministic behavior
pub struct DeterministicFs {
    /// In-memory filesystem state
    files: HashMap<PathBuf, FileState>,
    /// Write-ahead log for deterministic crash recovery testing
    wal: Vec<FsOperation>,
    /// Fault injection
    faults: Vec<DiskFault>,
    rng: DeterministicRng,
}

pub enum DiskFault {
    /// Simulate disk full
    DiskFull { after_bytes: u64 },
    /// Simulate I/O error on specific files
    IoError { path: PathBuf, probability: f64 },
    /// Simulate partial write (torn write)
    TornWrite { probability: f64 },
    /// Simulate fsync failure (data written but not durable)
    FsyncFailure { probability: f64 },
}
```

---

## 5. Snapshot, Fork, and Replay

### 5.1 Leveraging Hyperlight's Existing Snapshot System

Hyperlight already has a `Snapshot` struct that captures:
- Full guest memory (compacted via page table walk)
- Memory regions (extra mapped regions)
- Page table root address
- Special registers (CR3, etc.)
- Stack top address
- Blake3 hash for identity comparison

We extend this to also capture:
- Deterministic scheduler state
- Virtual clock value
- RNG state
- Network simulation state
- Filesystem state

```rust
/// Extended snapshot that includes all deterministic simulation state
pub struct SimulationSnapshot {
    /// Hyperlight's built-in VM snapshot
    vm_snapshot: hyperlight::Snapshot,
    /// Deterministic scheduler state
    scheduler: DeterministicScheduler,
    /// Virtual clock state
    clock: DeterministicClock,
    /// RNG state for reproducibility
    rng_state: DeterministicRngState,
    /// Network simulation state
    network: DeterministicNetwork,
    /// Filesystem state
    fs: DeterministicFs,
    /// Global simulation tick counter
    tick: u64,
}

impl SimulationSnapshot {
    /// Fork: create a new simulation from this snapshot with a different seed
    pub fn fork(&self, new_seed: u64) -> SimulationSnapshot {
        let mut forked = self.clone();
        forked.scheduler.reseed(new_seed);
        forked.rng_state.reseed(new_seed);
        forked
    }
}
```

### 5.2 Multi-Sandbox Orchestration

For testing distributed systems, we run **multiple sandboxes** as a simulated
cluster:

```rust
/// Orchestrates multiple deterministic sandboxes as a simulated cluster
pub struct SimulationCluster {
    /// All sandbox instances
    sandboxes: Vec<DeterministicSandbox>,
    /// Shared simulated network
    network: DeterministicNetwork,
    /// Shared virtual clock
    clock: DeterministicClock,
    /// Master RNG (each sandbox derives its own from this)
    master_rng: DeterministicRng,
    /// Global event queue (time-ordered)
    event_queue: BinaryHeap<Reverse<SimEvent>>,
    /// The master seed that determines everything
    master_seed: u64,
}

pub struct DeterministicSandbox {
    sandbox: hyperlight::MultiUseSandbox,
    scheduler: DeterministicScheduler,
    id: SandboxId,
    /// Local RNG derived from master
    rng: DeterministicRng,
}

#[derive(Ord, PartialOrd, Eq, PartialEq)]
pub struct SimEvent {
    time: VirtualTime,
    target: SandboxId,
    kind: SimEventKind,
}

pub enum SimEventKind {
    /// Deliver a network packet
    NetworkDeliver(Packet),
    /// Timer expired
    TimerFired(TimerId),
    /// Preemption quantum expired
    Preempt,
    /// Fault injection event
    InjectFault(Fault),
}
```

### 5.3 The Simulation Loop

```rust
impl SimulationCluster {
    /// Main simulation loop — fully deterministic given master_seed
    pub fn run(&mut self, max_ticks: u64) -> SimulationResult {
        for tick in 0..max_ticks {
            // 1. Advance clock to next event
            if let Some(event) = self.event_queue.pop() {
                self.clock.advance_to(event.0.time);
                self.dispatch_event(event.0);
            }

            // 2. For each sandbox, run one scheduling quantum
            let order = self.master_rng.shuffle(0..self.sandboxes.len());
            for idx in order {
                let sandbox = &mut self.sandboxes[idx];

                // Pick which thread to run
                if let Some(thread_id) = sandbox.scheduler.schedule_next() {
                    // Run the guest for one quantum
                    match sandbox.run_quantum(thread_id, &self.clock) {
                        QuantumResult::Completed => {},
                        QuantumResult::HostCall(call) => {
                            self.handle_host_call(idx, call);
                        }
                        QuantumResult::Blocked(reason) => {
                            sandbox.scheduler.block_current(reason);
                        }
                        QuantumResult::Crashed(error) => {
                            self.record_crash(idx, error);
                        }
                    }
                }
            }

            // 3. Process network — deliver ready packets
            self.network.deliver_ready_packets(
                self.clock.now(),
                &mut self.sandboxes,
            );

            // 4. Check test properties / invariants
            self.check_properties()?;

            // 5. Maybe inject faults
            if self.fault_engine.should_inject(tick, &self.master_rng) {
                self.fault_engine.inject_next_fault(
                    &mut self.sandboxes,
                    &mut self.network,
                );
            }
        }

        SimulationResult::Passed
    }
}
```

---

## 6. Fault Injection Engine

```rust
pub struct FaultInjectionEngine {
    /// Schedule of faults to inject
    fault_schedule: Vec<ScheduledFault>,
    /// Random fault generation
    rng: DeterministicRng,
    /// Fault injection strategies
    strategies: Vec<Box<dyn FaultStrategy>>,
}

pub trait FaultStrategy {
    /// Decide whether to inject a fault at this point
    fn should_inject(&self, state: &SimulationState) -> bool;
    /// Generate the specific fault to inject
    fn generate_fault(&mut self, rng: &mut DeterministicRng) -> Fault;
}

pub enum Fault {
    // Process-level faults
    KillSandbox(SandboxId),
    RestartSandbox(SandboxId),
    PauseSandbox(SandboxId, Duration),

    // Network faults
    NetworkPartition(Vec<SandboxId>, Vec<SandboxId>),
    NetworkHeal,
    SlowNetwork(SandboxId, Duration),

    // Disk faults
    DiskError(SandboxId, PathBuf),
    DiskFull(SandboxId),
    CorruptFile(SandboxId, PathBuf),

    // Resource faults
    MemoryPressure(SandboxId),
    CpuThrottle(SandboxId, f64),

    // Clock faults
    ClockSkew(SandboxId, i64), // For testing clock sync
    ClockJump(SandboxId, i64), // Sudden time jump
}
```

---

## 7. Concrete Implementation Plan

### Phase 1: Single-Sandbox Deterministic Execution

**Goal:** Run a single Hyperlight guest with fully deterministic behavior.

1. **Fork Hyperlight** and modify the KVM backend:
   - Pin TSC frequency and set initial TSC value
   - Disable RDRAND/RDSEED via CPUID filtering
   - Implement instruction counting via PMU for deterministic preemption

2. **Create `DeterministicSandbox` wrapper:**
   ```rust
   pub struct DeterministicSandbox {
       inner: MultiUseSandbox,
       clock: DeterministicClock,
       rng: DeterministicRng,
       seed: u64,
   }

   impl DeterministicSandbox {
       pub fn new(guest: GuestBinary, seed: u64) -> Result<Self> {
           let mut sandbox = UninitializedSandbox::new(guest, None)?;

           // Register deterministic host functions
           register_det_time(&mut sandbox, clock.clone());
           register_det_entropy(&mut sandbox, rng.clone());
           register_det_io(&mut sandbox, io.clone());

           let sandbox = sandbox.evolve()?;
           Ok(Self { inner: sandbox, clock, rng, seed })
       }

       pub fn call_deterministic(&mut self, func: &str, args: &[Value]) -> Result<Value> {
           // Same seed → same execution → same result
           self.inner.call_guest_function(func, args)
       }
   }
   ```

3. **Write a deterministic guest runtime crate:**
   - `det_runtime::time::now()` → calls host for virtual time
   - `det_runtime::random::bytes(n)` → calls host for seeded random
   - `det_runtime::thread::spawn(f)` → cooperative threading via host
   - `det_runtime::net::{send,recv}()` → calls host for simulated network

4. **Verify determinism:**
   - Run same guest with same seed 1000 times
   - Assert identical execution traces (memory state, return values, host
     call sequences)

### Phase 2: Multi-Sandbox Simulation Cluster

**Goal:** Simulate a distributed system with multiple sandboxes communicating
over a virtual network.

1. **Implement `SimulationCluster`** with the event-driven simulation loop
2. **Implement `DeterministicNetwork`** with configurable topology
3. **Implement snapshot/fork** for the entire cluster state
4. **Build basic fault injection** (kill sandbox, network partition)

### Phase 3: Intelligent Test Exploration

**Goal:** Automatically find bugs by exploring execution paths.

1. **Seed-based exploration** — Run many simulations with different seeds
2. **Coverage-guided exploration** — Use code coverage feedback to guide seed
   selection (like a fuzzer)
3. **Property checking** — Define invariants that must hold across all
   executions
4. **Minimization** — When a bug is found, binary-search for the minimal
   failing seed/schedule

### Phase 4: Production-Grade Tooling

1. **Execution recording and replay**
2. **Web UI for visualizing execution timelines**
3. **Integration with CI/CD**
4. **Support for testing existing protocols** (Raft, Paxos, etc.)

---

## 8. Key Hyperlight Modifications Required

### 8.1 KVM Backend Changes (`virtual_machine/kvm.rs`)

```rust
impl KvmVm {
    pub fn new_deterministic(config: DeterministicConfig) -> Result<Self> {
        let hv = Kvm::new()?;
        let vm_fd = hv.create_vm_with_type(0)?;
        let vcpu_fd = vm_fd.create_vcpu(0)?;

        // 1. Filter CPUID to disable non-deterministic features
        let mut cpuid = hv.get_supported_cpuid(KVM_MAX_CPUID_ENTRIES)?;
        Self::filter_cpuid_for_determinism(&mut cpuid);
        vcpu_fd.set_cpuid2(&cpuid)?;

        // 2. Pin TSC to fixed frequency
        vcpu_fd.set_tsc_khz(config.tsc_khz)?;

        // 3. Set up instruction counting for preemption
        // (via perf event or KVM PMU virtualization)

        // 4. Disable APIC timer (if present)
        // 5. Set deterministic initial register state

        Ok(Self { vm_fd, vcpu_fd })
    }

    fn filter_cpuid_for_determinism(cpuid: &mut CpuId) {
        for entry in cpuid.as_mut_slice() {
            match entry.function {
                1 => {
                    entry.ecx &= !(1 << 30); // Disable RDRAND
                    entry.ecx &= !(1 << 24); // Disable TSC-Deadline
                }
                7 if entry.index == 0 => {
                    entry.ebx &= !(1 << 18); // Disable RDSEED
                }
                0x15 => {
                    // TSC/Core Crystal Clock — report fixed values
                    entry.eax = 1;
                    entry.ebx = config.tsc_khz;
                    entry.ecx = 1000; // 1MHz reference
                }
                _ => {}
            }
        }
    }
}
```

### 8.2 VM Exit Handler Enhancement (`hyperlight_vm.rs`)

Extend the existing `run()` loop to intercept additional exit reasons:

```rust
// In the existing run() loop in hyperlight_vm.rs
match exit_reason {
    Ok(VmExit::Halt()) => break Ok(()),
    Ok(VmExit::IoOut(port, data)) => {
        // Existing: handles OutBAction (Log, CallFunction, Abort, DebugPrint)
        // New: route through deterministic controller
        self.det_controller.handle_io(port, data)?;
    }

    // NEW: Intercept RDTSC if configured to VM-exit on it
    Ok(VmExit::Rdtsc) => {
        let tsc = self.det_clock.now_as_tsc();
        let regs = self.vm.regs()?;
        self.vm.set_regs(&CommonRegisters {
            rax: tsc & 0xFFFFFFFF,
            rdx: tsc >> 32,
            rip: regs.rip + 2, // Skip past RDTSC instruction
            ..regs
        })?;
    }

    // NEW: Instruction count overflow → deterministic preemption
    Ok(VmExit::PmuOverflow) => {
        self.det_scheduler.preempt_current()?;
    }

    // Existing handlers...
    Ok(VmExit::MmioRead(addr)) => { /* ... */ }
    Ok(VmExit::MmioWrite(addr)) => { /* ... */ }
    Ok(VmExit::Cancelled()) => { /* ... */ }
}
```

### 8.3 Extended Snapshot (`sandbox/snapshot.rs`)

```rust
/// Extend the existing Snapshot to include simulation state
pub struct DeterministicSnapshot {
    /// The VM-level snapshot (memory, registers, page tables)
    vm_snapshot: Snapshot,
    /// Virtual clock state
    clock_nanos: u64,
    /// RNG state (for exact replay)
    rng_state: [u8; 32],
    /// Scheduler state
    scheduler_state: SchedulerSnapshot,
    /// Per-sandbox metadata
    sandbox_id: SandboxId,
}

/// Snapshot of scheduler state for exact replay
pub struct SchedulerSnapshot {
    ready_queue: VecDeque<ThreadId>,
    blocked_threads: HashMap<ThreadId, BlockReason>,
    current_thread: Option<ThreadId>,
    instruction_count: u64,
}
```

---

## 9. Comparison: Our Approach vs. Antithesis

| Aspect | Antithesis | Our Approach (Hyperlight) |
|--------|-----------|--------------------------|
| **Guest environment** | Full Linux VMs (unmodified binaries) | Bare-metal micro-VMs (compiled-to-Hyperlight) |
| **Deterministic scheduling** | Modified kernel scheduler in guest | Cooperative scheduler via host functions |
| **I/O interception** | Paravirtual drivers in custom kernel | `outb` port-based host functions (simpler) |
| **Snapshot granularity** | Full VM state (GB) | Micro-VM state (KB-MB, much faster) |
| **Fork speed** | Seconds (full VM clone) | Microseconds (tiny memory footprint) |
| **Multi-node simulation** | Multiple VMs on custom hypervisor | Multiple sandboxes in one process |
| **Code modification required** | None (runs unmodified binaries) | Compile against det. guest runtime |
| **Complexity** | Massive (custom hypervisor) | Moderate (extends existing VMM) |
| **Best for** | Testing existing distributed systems | Testing new systems designed for it |

**Our key advantage:** Hyperlight's micro-VM architecture means snapshots are
tiny (kilobytes instead of gigabytes), forks are near-instant, and we can run
thousands of simulations in parallel on a single machine. The tradeoff is
requiring code to compile against the deterministic guest runtime.

---

## 10. Example: Testing a Distributed KV Store

```rust
#[test]
fn test_kv_store_linearizability() {
    // Create a 3-node cluster simulation
    let mut cluster = SimulationCluster::new(ClusterConfig {
        master_seed: 42,
        num_nodes: 3,
        guest_binary: GuestBinary::FilePath("target/kv-store-guest.wasm"),
    });

    // Run 10,000 different executions with different seeds
    for seed in 0..10_000 {
        cluster.reset_with_seed(seed);

        // Concurrent operations from multiple "clients"
        cluster.schedule_client_op(0, Op::Put("key1", "value1"));
        cluster.schedule_client_op(1, Op::Put("key1", "value2"));
        cluster.schedule_client_op(2, Op::Get("key1"));

        // Inject faults during execution
        cluster.schedule_fault(
            100, // After tick 100
            Fault::NetworkPartition(vec![0], vec![1, 2]),
        );
        cluster.schedule_fault(
            200,
            Fault::NetworkHeal,
        );

        // Run simulation
        let result = cluster.run(1000);

        // Check linearizability
        assert!(
            is_linearizable(&result.history),
            "Linearizability violated with seed {seed}: {:?}",
            result.history
        );
    }
}
```

---

## 11. Open Questions and Research Areas

1. **Instruction-precise preemption on KVM** — Can we get exact instruction
   counting via PMU virtualization? Or do we need single-stepping (slow)?
   Alternative: rely purely on cooperative yields at known points.

2. **RDTSC interception overhead** — VM exits for RDTSC are expensive. Can we
   use TSC offsetting instead (set a fixed TSC value before each run)?

3. **Multi-vCPU determinism** — Hyperlight is single-vCPU. For testing
   multi-threaded code, we must either:
   - (a) Run all threads cooperatively on one vCPU (our approach), or
   - (b) Implement deterministic multi-vCPU scheduling (much harder)

4. **Compatibility layer** — How much of a standard libc/POSIX API can we
   provide through the deterministic guest runtime? Can we compile existing
   Rust/C code with minimal changes?

5. **WASM as guest format** — Could we use WASM→native compilation as the
   guest format? WASM is inherently more deterministic (no direct hardware
   access), and Hyperlight could execute compiled WASM.

6. **Coverage-guided seed selection** — Can we use AFL/libfuzzer-style
   coverage feedback to guide which scheduling seeds to explore next?

---

## 12. Gap Analysis — What the Design is Missing

After auditing Hyperlight's actual source code against the design, here are the
significant gaps, ranked by severity.

---

### 🔴 CRITICAL: Floating-Point Non-Determinism Across CPU Microarchitectures

**The problem nobody talks about.** The design assumes that identical inputs +
identical seed = identical outputs. But x86 floating-point behavior varies
between CPU microarchitectures:

- **Denormalized number handling** — Intel and AMD handle denorms differently.
  The `FTZ` (Flush-to-Zero) and `DAZ` (Denormals-are-Zero) bits in `MXCSR`
  affect results. Hyperlight resets MXCSR to `0x1F80` (default) on each
  `dispatch_call_from_host()`, but this doesn't guarantee identical FP results
  across different CPUs.
- **x87 80-bit vs SSE 64-bit precision** — x87 uses 80-bit extended precision
  internally, SSE uses 64-bit. The compiler's choice of x87 vs SSE for the
  same code produces different rounding results.
- **`FSIN`/`FCOS`/`FPTAN` transcendental instructions** — These are only
  accurate to ~66 bits on Intel, and precision varies by microarchitecture.
  Different CPU steppings give different results for the same inputs.
- **AVX/AVX-512 upper bits** — VZEROUPPER behavior, transition penalties, and
  zero-extension semantics vary.

**The fix:** Pin the CPUID to report a specific (minimal) feature set and force
`FTZ+DAZ` mode. Or: run all simulations on identical hardware (what Antithesis
does). Hyperlight's guest context save uses `fxsave` (512-byte legacy), which
doesn't capture AVX/AVX-512 state at all — they noted this as a TODO in
`context.rs`:

```
// TODO: Check if we ever generate code with ymm/zmm in
//       the handlers and save/restore those as well
```

**Mitigation:**
```rust
fn pin_fp_determinism(cpuid: &mut CpuId) {
    // Force SSE-only (disable AVX, AVX2, AVX-512)
    for entry in cpuid.as_mut_slice() {
        if entry.function == 1 {
            entry.ecx &= !(1 << 28); // Disable AVX
        }
        if entry.function == 7 && entry.index == 0 {
            entry.ebx &= !(1 << 5);  // Disable AVX2
            entry.ebx &= !(1 << 16); // Disable AVX-512F
        }
    }
    // Set MXCSR with FTZ+DAZ for deterministic denorm handling
    // FTZ = bit 15, DAZ = bit 6
    const MXCSR_DETERMINISTIC: u32 = 0x1F80 | (1 << 15) | (1 << 6);
}
```

---

### 🔴 CRITICAL: No RDTSC Interception Exists — And It's Hard

The design claims we can intercept `rdtsc`. But:

1. **Hyperlight's KVM backend does NOT set up RDTSC exiting.** There's zero
   code for `KVM_CAP_TSC_CONTROL`, TSC frequency pinning, or RDTSC exit
   handling. The `VmExit` enum doesn't even have an `Rdtsc` variant.

2. **KVM RDTSC interception is expensive and tricky.** You need to:
   - Set the "RDTSC exiting" bit in the VMCS primary processor-based controls
   - KVM doesn't easily expose this — you'd need `KVM_CAP_TSC_CONTROL` + 
     custom VMCS configuration
   - Each RDTSC-exit costs ~1-3µs (VM exit + entry), and RDTSC is called
     frequently in performance-sensitive code

3. **The guest tracing code ALREADY uses `rdtsc` directly!**
   (`hyperlight_guest_tracing/src/invariant_tsc.rs` calls `_rdtsc()`)

**Better approach:** Don't intercept RDTSC at exit time. Instead:
- Use `KVM_SET_TSC_KHZ` to pin TSC frequency (KVM does support this)
- Use the TSC offset field in VMCS to set a known starting value
- Accept that TSC reads return *cycle counts* not wall time, and ensure
  the guest never converts TSC to wall time without going through a host
  function
- Disable invariant TSC reporting in CPUID so guests don't rely on it

---

### 🔴 CRITICAL: The Cooperative Threading Model Is a Fantasy (For Real Code)

The design proposes cooperative threading where guest code calls
`det_thread_create()`, `det_mutex_lock()`, etc. But:

1. **Hyperlight guests have NO threading support.** Zero. There's no
   `pthread`, no thread-local storage, no context switching. The guest runs
   as a single-threaded `#[no_std]` Rust program.

2. **Building a cooperative threading runtime is a major project** — You need:
   - User-space context switching (save/restore all registers, stack pointer)
   - A stack allocator (each "thread" needs its own stack)
   - All blocking primitives (mutex, condvar, channel, semaphore)
   - Timer management for sleep/timeout
   - TLS (thread-local storage) emulation

3. **Existing Rust async runtimes won't work.** Tokio, async-std, smol all
   require `std`, system calls, epoll/kqueue, etc. You'd need a custom
   async runtime built for `#[no_std]` Hyperlight guests.

4. **The PMU-based preemption the design proposes doesn't exist in KVM's API
   in a way that's easily accessible.** `KVM_SET_GUEST_DEBUG` with 
   single-stepping works but is ~100x slower. There is no `VmExit::PmuOverflow`
   in Hyperlight or KVM.

**What actually works:** Build a deterministic `#[no_std]` async runtime:
```rust
// This is the realistic path:
// 1. Cooperative async tasks (Rust futures) on a single thread
// 2. Deterministic executor that polls futures in seed-determined order
// 3. Yield points are natural: every .await is a yield
pub struct DeterministicExecutor {
    tasks: Vec<Pin<Box<dyn Future<Output = ()>>>>,
    rng: DeterministicRng,
}

impl DeterministicExecutor {
    fn run_until_idle(&mut self) {
        // Poll tasks in deterministic order
        let order = self.rng.shuffle_indices(self.tasks.len());
        for idx in order {
            let waker = noop_waker(); // or real waker
            let mut cx = Context::from_waker(&waker);
            let _ = self.tasks[idx].as_mut().poll(&mut cx);
        }
    }
}
```

---

### 🟡 MAJOR: Snapshot Doesn't Capture Host-Side State

The design extends `Snapshot` with simulation state, but Hyperlight's snapshot
only captures **guest VM state** (memory + registers). It does NOT capture:

- **Host function registry state** — If host functions have closures with
  mutable state
- **The `FunctionRegistry`** — Which is `Arc<Mutex<FunctionRegistry>>`
- **Any host-side buffers** for network simulation, filesystem, etc.

For true snapshot/restore of a simulation, you need to serialize ALL of:
```
Guest VM state (Hyperlight handles this)
+ Scheduler state
+ Virtual clock
+ RNG state  
+ Network simulation queues
+ Filesystem state
+ Host function closure state  ← THIS IS THE HARD ONE
```

Host function closures can capture arbitrary Rust state. You'd need to either:
- Forbid stateful host functions (only pure functions)
- Require all host-side state to implement `Serialize`/`Deserialize`
- Use a state object pattern where all mutable state lives in a single
  serializable struct

---

### 🟡 MAJOR: Cross-Machine Reproducibility Is Not Guaranteed

The design implies "same seed = same execution." But:

1. **Different CPUs report different CPUID** — which can cause different code
   paths (e.g., `has_avx2()` → use SIMD vs scalar path)
2. **Different KVM versions** behave differently for edge cases
3. **Different `rustc` versions** produce different codegen
4. **The buddy allocator (`LockedHeap<32>`)** is deterministic given the same
   initial heap, but allocation patterns vary with compiler changes

**Fix:** Record a "platform fingerprint" with each test run:
```rust
struct PlatformFingerprint {
    cpuid_leaves: Vec<CpuidLeaf>,  // What the guest actually sees
    kvm_version: u32,
    guest_binary_hash: [u8; 32],   // Blake3 of the compiled guest
    hyperlight_version: String,
}
```
Reproducibility is only guaranteed with identical fingerprints.

---

### 🟡 MAJOR: No Plan for Existing Code / Compatibility Layer

Antithesis's power is running **unmodified binaries**. Our design requires code
to be:
- Compiled as `#[no_std]` Rust
- Linked against a custom guest runtime
- Using only deterministic host functions for I/O

This means you can't test:
- Existing services (they use `std`, tokio, serde, etc.)
- C/C++ code (no libc available)
- Go/Java/Python services

**Missing: A compatibility strategy.** Options:
1. **`hyperlight-wasm` path** — Compile to WASM, run in Hyperlight. The
   Hyperlight codebase already references `hyperlight-wasm` (sandbox traits
   mention it). WASM is inherently more deterministic and has a rich ecosystem.
2. **Minimal `std` shim** — Implement enough of `std` (backed by host
   functions) for common Rust crates to compile
3. **musl-libc port** — Port musl to Hyperlight's environment for C compat
4. **Accept the limitation** — Position this as a tool for new systems designed
   for deterministic testing (like FoundationDB was)

---

### 🟡 MAJOR: Multi-Sandbox Coordination Has No Defined Ordering Semantics

The design shows a simulation loop that runs sandboxes in "shuffled order," but
doesn't address:

1. **What happens when sandbox A sends a message and sandbox B receives it in
   the same tick?** Is this allowed? Must delivery be deferred to the next tick?
2. **Causal ordering** — If A→B→C is a causal chain, the simulation must
   respect this regardless of scheduling order
3. **How do you handle a sandbox that blocks waiting for a network message?**
   It's running on a single vCPU — you can't literally block. You need to:
   - Return from the guest to the host (via `hlt` or host function)
   - Record that this sandbox is "blocked on recv"
   - Only re-enter it when a message arrives
4. **Deadlock detection** — When all sandboxes are blocked, is that a bug or
   normal (waiting for timer)?

---

### 🟢 MODERATE: XSAVE State Incomplete for Cooperative Thread Switching

The guest runtime's context save uses `fxsave` (512 bytes), which saves x87 +
SSE state. But it does NOT save:
- AVX state (YMM upper halves)
- AVX-512 state (ZMM registers, k-mask registers)
- MPX bounds registers
- PKRU state

If you disable AVX via CPUID filtering (recommended above), this is fine. But
the design should explicitly state that the guest must be compiled with
`-C target-feature=-avx,-avx2,-avx512f` or equivalent.

---

### 🟢 MODERATE: No Execution Recording Format

For replay, you need a compact log of all non-deterministic choices:
- Which thread was scheduled at each decision point
- What values were returned by host functions (time, random, network recv)
- What faults were injected and when

The design mentions "replay" but doesn't define the recording format. This is
critical for:
- Reproducing bugs on different machines
- Minimizing failing test cases
- Debugging (replay with breakpoints)

```rust
/// Compact execution recording for deterministic replay
pub struct ExecutionRecording {
    /// Platform fingerprint for reproducibility checking
    platform: PlatformFingerprint,
    /// Master seed (in theory, this IS the recording — everything
    /// else can be derived from it if platform matches)
    master_seed: u64,
    /// But in practice, also record decisions for cross-platform replay:
    decisions: Vec<SchedulingDecision>,
    /// Host function return values (for replay without live host)
    host_returns: Vec<HostFunctionReturn>,
}
```

---

### 🟢 MODERATE: State Space Explosion Not Addressed

With N tasks and M scheduling points, there are N^M possible schedules. The
design mentions "coverage-guided seed selection" but doesn't address:

1. **How to define "interesting" coverage** — Code coverage? State coverage?
   Schedule coverage?
2. **Partial-order reduction** — Most scheduling choices are equivalent
   (independent actions commute). Smart exploration can reduce the space
   exponentially.
3. **DPOR (Dynamic Partial Order Reduction)** — Well-studied technique for
   reducing schedule exploration
4. **Bounded model checking** — Limit exploration depth and thread preemption
   count

---

### 🟢 MODERATE: Test Oracle Problem

The design shows a linearizability check but doesn't address:

1. **Linearizability checking is NP-complete** in the general case
2. **Most teams don't know what properties to check** — need a property
   specification language or common patterns
3. **Liveness properties** (something eventually happens) are fundamentally
   different from safety properties (something bad never happens) and require
   different checking strategies
4. **Performance properties** can't be checked in simulated time

---

### 🟢 MODERATE: Missing — How to Handle Guest Panics and Stack Unwinding

Hyperlight guests have a panic handler that calls `write_abort()` → host
receives `OutBAction::Abort`. But for simulation:

1. **Should a panic in one sandbox be treated as a test failure?** Or is it an
   expected fault the system should handle?
2. **Stack unwinding** — Hyperlight uses `panic = "abort"` (no unwinding). How
   do you test code that relies on `Drop` for cleanup?
3. **How do you capture the panic backtrace** for debugging?

---

### 🟢 MINOR: Host-Side Non-Determinism

The simulation controller itself runs on a normal host OS. Sources of host-side
non-determinism that could leak in:

1. **`HashMap` iteration order** — Rust's `HashMap` uses random hashing by
   default. Any `HashMap` in the controller must use a deterministic hasher
   (e.g., `ahash` with fixed seed or `BTreeMap`).
2. **`Arc<Mutex<>>` lock acquisition order** — The host function registry uses
   `Arc<Mutex<FunctionRegistry>>`. If multiple host threads access this, lock
   order is non-deterministic. (Hyperlight is single-threaded per sandbox, so
   this is fine in practice.)
3. **Floating-point in the controller** — If the fault injection engine uses
   floating-point probabilities, host FP non-determinism leaks in.

**Fix:** Use `BTreeMap` everywhere in the controller, avoid `f64` for
scheduling decisions, use integer-based probability calculations.

---

### Summary Table

| Gap | Severity | Effort to Fix | Risk if Ignored |
|-----|----------|--------------|-----------------|
| FP non-determinism across CPUs | 🔴 Critical | Medium | Silent wrong results |
| No RDTSC interception | 🔴 Critical | High | Time-dependent code varies |
| Cooperative threading is fantasy | 🔴 Critical | Very High | Can't test concurrent code |
| Snapshot misses host state | 🟡 Major | Medium | Broken replay |
| Cross-machine reproducibility | 🟡 Major | Medium | "Works on my machine" |
| No compatibility layer | 🟡 Major | Very High | Limited adoption |
| Multi-sandbox ordering | 🟡 Major | Medium | Simulation bugs |
| XSAVE incomplete | 🟢 Moderate | Low | Fix with CPUID filter |
| No recording format | 🟢 Moderate | Medium | Can't share/replay bugs |
| State space explosion | 🟢 Moderate | High | Slow to find bugs |
| Test oracle problem | 🟢 Moderate | High | Don't know what to check |
| Guest panic handling | 🟢 Moderate | Low | Confusing test failures |
| Host-side non-determinism | 🟢 Minor | Low | Rare subtle issues |

---

## 13. Revised Approach — Taking the Gaps Seriously

Given the gap analysis, here's the pragmatic path forward:

### The Core Insight

**Don't try to be Antithesis.** Antithesis runs unmodified binaries in full VMs
with a custom hypervisor — that's a 100-person-year effort. Instead, build a
**FoundationDB-style simulation testing framework** that leverages Hyperlight's
isolation as a bonus, not a requirement.

### Revised Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                    Deterministic Simulation                       │
│                                                                   │
│   Level 1: Pure Simulation (no VMs)                              │
│   ├── Deterministic async runtime (#[no_std] compatible)         │
│   ├── Simulated network (in-process message passing)             │
│   ├── Simulated disk (in-memory VFS)                             │
│   ├── Virtual clock                                              │
│   └── Seeded RNG                                                 │
│                                                                   │
│   Level 2: Hyperlight Isolation (optional hardening)             │
│   ├── Each simulated node runs in a Hyperlight sandbox           │
│   ├── Fault isolation (one node's crash doesn't kill others)     │
│   ├── Memory isolation (no accidental shared state)              │
│   └── Snapshot/restore for forking execution paths               │
│                                                                   │
│   Level 3: Intelligent Exploration                               │
│   ├── Seed-based fuzzing of scheduling decisions                 │
│   ├── Coverage-guided exploration                                │
│   ├── DPOR for partial-order reduction                           │
│   └── Property-based oracles                                     │
└──────────────────────────────────────────────────────────────────┘
```

**Key insight:** Level 1 works WITHOUT Hyperlight. You can build and iterate on
the deterministic simulation framework as a pure Rust library. Hyperlight
(Level 2) adds isolation and snapshot/fork capabilities. This de-risks the
project enormously.

### Revised Phase 1: Deterministic Async Runtime (No VMs Yet)

Build a `#[no_std]`-compatible deterministic async executor:

```rust
/// Works both in normal Rust AND inside Hyperlight guests
#[cfg_attr(not(feature = "std"), no_std)]
pub mod det_runtime {
    pub struct Executor { /* ... */ }
    pub struct SimNetwork { /* ... */ }
    pub struct SimClock { /* ... */ }
    pub struct SimFs { /* ... */ }
}

// Test like this — no VMs needed:
#[test]
fn test_raft_consensus() {
    let mut sim = Simulation::new(seed: 42);
    let nodes = sim.spawn_nodes::<RaftNode>(3);
    sim.run_until(|s| s.has_leader());
    sim.inject(Fault::Partition([0], [1, 2]));
    sim.run_for(Duration::from_secs(10));
    assert!(sim.has_leader());
}
```

### Revised Phase 2: Hyperlight Sandbox Integration

Once the runtime works, optionally run each node in a Hyperlight sandbox:

```rust
// Same test, but with VM isolation:
#[test]
fn test_raft_with_isolation() {
    let mut sim = Simulation::new(seed: 42)
        .with_hyperlight_isolation(true);  // Each node gets a sandbox
    // ... same test code ...
}
```

### Revised Recommended First Steps

1. **Build the deterministic async executor** — `no_std`, runs Rust futures
   in deterministic (seed-based) order. Test it without any VMs.

2. **Build the simulated network** — In-process message passing with
   deterministic ordering, latency simulation, and partition injection.

3. **Prove determinism** — Run a simple distributed algorithm (echo server,
   2PC) 10,000 times with same seed, verify identical results.

4. **Add Hyperlight isolation** — Run each node in a sandbox. Verify
   determinism is preserved across the VM boundary.

5. **Build RDTSC handling** — Pin TSC frequency via KVM, verify
   time-dependent code is deterministic within a single CPU model.

6. **Build the explorer** — Seed-based fuzzing, then coverage-guided
   exploration, then DPOR.

7. **Build the fault injector** — Process kill, network partition, disk
   error, clock skew.

8. **Build the recorder/replayer** — Log scheduling decisions, enable
   exact replay of failing tests.
