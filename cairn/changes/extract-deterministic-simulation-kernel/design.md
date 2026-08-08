## Context

`SimulationController` (3344 lines) imports `kvm_bindings` and `kvm_ioctls` directly and drives `VmFd`/`VcpuFd` while it also decides tick advancement, vCPU scheduling, network delivery, and fault injection. `vm.rs` (4170 lines), `cpu.rs`, and `memory.rs` own the KVM machine. The `verified/` module and the `chaoscontrol-replay-evidence-core` extraction show the repo's direction: pure deterministic cores, thin effect shells.

Today a deterministic schedule cannot execute without KVM. The evidence crate's in-process simulator duplicates scheduling, clock, network, and fault concepts for the same reason.

## Boundary inventory

| Surface | Classification | Authority after extraction |
| --- | --- | --- |
| `scheduler.rs` and `scheduler/core.rs` | Pure core | `chaoscontrol-sim-core::scheduler`; the VMM path is a compatibility re-export. |
| Round tick, virtual time, VM selection, command sequence, and canonical round trace | Pure core | `chaoscontrol-sim-core::kernel`; the VMM supplies explicit VM status and applies admitted commands. |
| Execution commands, exit observations, sequence checks, and DTO bounds | Pure core | `chaoscontrol-sim-core::boundary`. |
| Network routing, seeded packet mutation, delivery ordering, and network fault state | Core candidate | The current implementation remains in `controller.rs` until its fault-outcome coupling is separated. It is not classified as a machine effect. |
| Fault schedule selection and application planning | Existing pure core dependency | `chaoscontrol-fault`; the simulation core consumes its typed plans rather than duplicating policy. |
| Scheduler snapshot state and validation | Pure core | Relocated with the scheduler. Complete VM/device snapshot DTOs remain in the shell because they contain KVM ABI and device types. |
| `SimulationController`, VM creation, kernel loading, dlog paths, and orchestration | Imperative shell | `chaoscontrol-vmm::controller`. |
| `vm.rs`, `cpu.rs`, `memory.rs`, `registers.rs`, `snapshot.rs`, and `devices/` | Imperative shell | KVM ioctls, guest memory/register access, MMIO, interrupts, and capture/restore effects remain in `chaoscontrol-vmm`. |
| `chaoscontrol-explore` workers and CLI | Imperative shell | Existing VMM paths remain stable through compatibility adapters. |

The remaining extraction boundary is explicit: network decisions must move after their evidence-ledger coupling is split. VM and device snapshot effects cannot move into the pure crate.

## Decisions

### 1. One new pure core crate, extracted incrementally

**Choice:** Create `crates/chaoscontrol-sim-core`. Move deterministic decision logic out of `chaoscontrol-vmm` in strangler steps: scheduler, network fabric, virtual clock, fault schedule selection, event/trace model, then the tick loop. Each step keeps the workspace green.

**Rationale:** A big-bang rewrite of a 3000-line controller risks silent behavioral drift. Incremental moves keep every intermediate state testable against the existing determinism suites.

### 2. Command/observation execution boundary

**Choice:** The core never calls a machine. It consumes `ExitObservation` values (halt, MMIO, I/O port, snapshot request, assertion event) and returns `ExecutionCommand` values (run vCPU, write registers, deliver interrupt, inject fault). The KVM shell maps commands to ioctls and exit reasons to observations. No async, no callbacks into the core.

**Rationale:** A request/response boundary keeps the core free of lifetimes over file descriptors, guest memory handles, and inversion of control. It also gives non-KVM executors (in-process simulator, test doubles) a single implementable contract.

### 3. Determinism is proven by trace equivalence, not inspection

**Choice:** Migration acceptance requires seeded equivalence: for a fixed seed, config, and guest artifact set, the pre-split and post-split engines emit identical canonical event traces. Add negative fixtures that mutate one input and require a detected divergence.

**Rationale:** The product claim is deterministic replay. Only a trace comparison over real runs demonstrates that the split moved code without changing decisions.

### 4. Snapshot models relocate without semantic change

**Choice:** Pure snapshot state types and their validation move to the core only after, or in coordination with, the active `complete-vm-snapshot-state` change. This change does not add, remove, or reinterpret any snapshot field. Capture and restore ioctls stay in the shell.

**Rationale:** Two changes must not own the same semantics. Relocation is mechanical; completeness is the other change's authority.

### 5. No new evidence claims

**Choice:** The split is an internal architecture change. CLI output, artifact formats, verdict JSON, and Nickel contracts keep their current shapes. Receipts may note the core crate as the DTO/decision authority, as they already do for `chaoscontrol-replay-evidence-core`.

**Rationale:** Claim stability lets existing dogfood evidence and readiness gates validate the migration without fixture rewrites.

## Risks / Trade-offs

- Controller coupling depth: exit handling and device state are interleaved with scheduling decisions. Mitigation: strangler extraction with equivalence tests at each step, not a single move.
- Command/observation overhead: one round trip per vCPU exit adds allocation. Mitigation: batch commands per tick; measure the existing exploration benchmark before and after.
- Semantic drift during moves: mitigated by seeded trace equivalence as a hard gate, not a smoke check.
