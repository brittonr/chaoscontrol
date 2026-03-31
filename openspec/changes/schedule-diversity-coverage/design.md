## Context

The explorer currently treats vCPU scheduling as a fixed parameter. You pick `--scheduling round-robin` or `--scheduling randomized` at startup and every branch in every round uses that same config. The `VcpuScheduler` is seeded once per VM and its state is snapshot/restored along with the VM — meaning all branches forked from the same snapshot replay the same interleaving.

For SMP guests, this means the explorer only varies *what faults happen* and *what random choices the guest makes*, but never *in what order the vCPUs execute*. Thread interleaving is a third axis of exploration that's currently locked.

The coverage bitmap (64KB AFL-style) tracks code edges and assertion states. It has no signal for "this branch ran vCPU 0 for 200 exits then switched to vCPU 1" vs "this branch alternated every 10 exits." Both look identical to the coverage collector.

## Goals / Non-Goals

**Goals:**
- Branches within a round explore different vCPU interleavings, not just different faults.
- The coverage bitmap distinguishes branches that took different scheduling paths, keeping the frontier alive longer for SMP workloads.
- Schedule diversity works alongside existing fault-schedule and input-tree exploration modes.
- Determinism preserved — every branch's interleaving is reproducible from its schedule seed.

**Non-Goals:**
- Systematic schedule enumeration (PCT, iterative context bounding). This is coverage-guided random variation, not model checking.
- Per-instruction interleaving control. Granularity stays at VM-exit level (quantum of exits).
- Changing single-vCPU behavior. When `num_vcpus == 1`, schedule diversity is a no-op.

## Decisions

### 1. Schedule fingerprint via transition hashing

**Decision**: The `VcpuScheduler` accumulates a rolling hash of `(active_vcpu, quantum_length)` pairs as it runs. After a branch completes, this fingerprint is injected into the coverage bitmap.

**Why not hash every exit?** Exit-level hashing would overflow the bitmap (thousands of exits per branch). Transition-level hashing tracks the *structure* of the interleaving — how many exits each vCPU got before switching — which is what matters for concurrency bugs. A quantum-10 schedule with 5 switches looks different from a quantum-100 schedule with 1 switch.

**Implementation**: `VcpuScheduler` gets a `fingerprint: u64` field, updated on each `advance()` call: `fingerprint = fingerprint.wrapping_mul(0x517cc1b727220a95) ^ (active as u64) ^ (quantum as u64)`. Snapshot/restore includes the fingerprint. After a branch, the explorer hashes the fingerprint into 4-8 coverage bitmap slots (similar to assertion-state enrichment using the upper half of MAP_SIZE).

### 2. Per-branch scheduler config via `ScheduleVariant`

**Decision**: Introduce a `ScheduleVariant` struct that overrides the scheduler's seed and strategy for a single branch. The explorer generates these alongside fault schedule variants.

```rust
struct ScheduleVariant {
    /// Scheduler RNG seed for this branch.
    scheduler_seed: u64,
    /// Override strategy (None = use config default).
    strategy_override: Option<SchedulingStrategy>,
    /// Override quantum (None = use config default).
    quantum_override: Option<u64>,
}
```

**Why per-branch, not per-round?** Per-round variation means all branches in a round share the same interleaving — you'd need many rounds to get diversity. Per-branch means every branch in a single round explores a different interleaving, multiplying exploration surface by `branch_factor`.

**Why not mutate the scheduler state inside the snapshot?** Snapshots capture the scheduler's RNG position. Mutating inside the snapshot would require deserializing, modifying, and re-serializing scheduler state in the snapshot blob. Applying a fresh `ScheduleVariant` after restore is simpler and equally effective — the scheduler starts from a known state with a new seed.

### 3. Schedule mutations in `ScheduleMutator`

**Decision**: Add three new mutation operators to `ScheduleMutator`:
- **ReSeed**: Replace the scheduler seed (different interleaving, same faults).
- **QuantumShift**: Multiply or divide the quantum by 2-8× (coarser or finer interleaving granularity).
- **StrategyFlip**: Switch between RoundRobin and Randomized.

These are combined with existing fault mutations. When schedule diversity is enabled, ~30% of mutations target scheduling, ~70% target faults. The ratio is configurable.

**Why bake this into the mutator rather than a separate layer?** The mutator already handles "generate N variant inputs for this round." Adding schedule variants as another mutation axis keeps the code path unified — one `BranchWork` struct carries both the fault schedule and the schedule variant.

### 4. Fingerprint goes in coverage bitmap upper quarter

**Decision**: Schedule fingerprint edges occupy bitmap indices `[3/4 * MAP_SIZE, MAP_SIZE)` (16KB). Assertion-state edges use `[MAP_SIZE/2, 3/4 * MAP_SIZE)` (16KB). Code edges use `[0, MAP_SIZE/2)` (32KB).

This gives each signal its own region to avoid hash collisions across domains. The code edge region shrinks from 64KB to 32KB, but the Raft guest only produces ~15K-29K edges total — well within 32KB.

### 5. Controller gets `apply_schedule_variant`

**Decision**: `SimulationController` gets a method that re-seeds all per-VM schedulers from a `ScheduleVariant` after snapshot restore. This is called by `run_branch` between restore and run.

```rust
impl SimulationController {
    fn apply_schedule_variant(&mut self, variant: &ScheduleVariant) { ... }
}
```

Each VM's scheduler gets `variant.scheduler_seed + vm_id` as its seed (domain separation per VM). Strategy and quantum overrides apply to all VMs uniformly.

## Risks / Trade-offs

**[Risk] Bitmap region shrinkage** → 32KB for code edges may cause more hash collisions in code-heavy guests. Mitigation: monitor `count_bits()` vs MAP_SIZE/2; if saturation exceeds 50%, the bitmap is too small regardless.

**[Risk] Schedule fingerprint collisions** → Two meaningfully different interleavings could hash to the same fingerprint. Mitigation: the rolling hash has 64 bits of state; mapping to 16KB of bitmap via multiple slots makes accidental collision unlikely for the branch counts we run (~16-64 per round).

**[Risk] Schedule diversity hides fault-sensitivity** → A bug found under a specific interleaving + specific fault combination may be harder to minimize because the minimizer only reduces faults, not scheduling. Mitigation: `ScheduleVariant` is recorded in `BranchResult` and `BugReport`, so reproduction replays the exact interleaving. Minimizer can optionally hold scheduling fixed while reducing faults.

**[Risk] Single-vCPU overhead** → Schedule diversity code paths execute but produce no useful signal with 1 vCPU. Mitigation: fingerprint is always 0 when `num_vcpus == 1` (scheduler never advances), so the bitmap slots stay empty. No false coverage signal, minimal overhead (one hash operation per branch).
