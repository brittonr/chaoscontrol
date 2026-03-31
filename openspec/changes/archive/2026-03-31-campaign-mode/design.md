## Context

The existing `Explorer` runs a single seed: bootstrap once, then loop rounds of snapshot-restore-mutate-run-collect. The `WorkerPool` parallelizes branches *within* a round (same seed, same snapshot, different schedules). This works well for branch-level throughput but doesn't help when the exploration strategy itself plateaus — which it does after 1-2 rounds for protocol-state-driven guests like Raft.

The napkin finding is clear: independent seeds with long runs find more rare bugs than deep single-seed exploration. The 9950X3D has 16 cores. A 3-VM exploration uses 3 KVM FDs. We can run ~5 independent seeds simultaneously, each with its own `Explorer`.

## Goals / Non-Goals

**Goals:**
- Run N independent explorations with seeds 0..N (or user-specified seed list) in parallel.
- Aggregate bugs across seeds with deduplication (same assertion_id + fault type set = one bug).
- Produce a unified report: per-seed summaries + merged bugs + union coverage stats.
- Machine-readable `campaign_report.json` for CI integration.
- Reuse existing `Explorer` unchanged — campaign mode is pure composition.

**Non-Goals:**
- Cross-seed coverage sharing (seeds are independent; no global coverage bitmap federation).
- Adaptive seed allocation (e.g., killing low-yield seeds to give cores to productive ones). Future work.
- Distributed campaigns across multiple machines. Single-host only.
- Changing the `Explorer` or `WorkerPool` internals.

## Decisions

### 1. Thread-per-seed with scoped threads

Each seed runs in its own OS thread via `std::thread::scope`. The `Explorer` is created, configured, and run entirely within the thread. No shared mutable state between seeds.

**Why not processes?** Threads share the kernel path and initrd in memory (read-only). Processes would each read the files independently. Threads also make result aggregation trivial — just collect `ExplorationReport` values at join.

**Why not async?** `Explorer::run()` is a blocking loop that calls KVM ioctls. Async would add complexity with no benefit — the parallelism is CPU-bound, not I/O-bound.

**SIGALRM isolation:** SMP VMs use `ITIMER_REAL` which is process-wide. Campaign mode sets `num_workers: 1` per seed (no within-round parallelism) to avoid SIGALRM conflicts between threads. Each seed's Explorer runs branches sequentially on its thread. The parallelism comes from running multiple seeds, not multiple branches within a seed. If the user also passes `--workers`, campaign mode ignores it with a warning.

### 2. Seed generation

Default: seeds `base_seed, base_seed+1, ..., base_seed+N-1` where `base_seed` is the `--seed` flag (default 42). This is simple, reproducible, and sufficient. The user can also provide an explicit seed list via `--seeds 42,99,137`.

### 3. Output directory structure

```
output/
  campaign_report.json    # aggregated machine-readable report
  campaign_report.txt     # aggregated human-readable report
  assertions.json         # merged assertion details
  seed_42/                # per-seed output
    report.txt
    assertions.json
    checkpoint.json
    bug_0.json
    bug_0.txt
  seed_43/
    ...
```

Each seed's `Explorer` gets `output_dir = output/seed_{N}`. Campaign-level aggregation reads back the per-seed reports after all seeds complete.

### 4. Bug deduplication across seeds

Same strategy as within-seed dedup: hash(assertion_id, sorted fault type names). If seeds 42 and 43 both find "leader completeness" violated via `[NetworkPartition, ProcessKill]`, that's one bug in the campaign report with a note about which seeds triggered it.

New: `CampaignBug` wraps a `SerializableBug` plus `found_by_seeds: Vec<u64>` and `first_seed: u64`.

### 5. Report aggregation

`CampaignReport` contains:
- `seeds_run: Vec<u64>` — which seeds ran
- `seeds_with_bugs: Vec<u64>` — which found at least one bug
- `total_rounds: u64` — sum across seeds
- `total_branches: u64` — sum across seeds
- `bugs: Vec<CampaignBug>` — deduplicated across seeds
- `per_seed: Vec<SeedSummary>` — per-seed stats (rounds, branches, edges, bugs, time)
- `assertion_details: Vec<AssertionDetail>` — merged across seeds (sum counts, worst verdict)
- `wall_clock_seconds: f64`

The human-readable report shows a per-seed table, then the merged bug list, then the merged assertion verdicts.

### 6. Core affinity

Campaign threads don't set core affinity by default. The OS scheduler handles thread-to-core mapping. If `--base-core` is set, seed i's Explorer gets `base_core = base + (i * num_vms)` so VMs from different seeds don't share cores.

### 7. Progress reporting

Each seed's Explorer logs independently via `env_logger` (thread-safe). Campaign mode prints a summary line after each seed completes:

```
[seed 42] done: 10 rounds, 80 branches, 256 edges, 1 bug (23.4s)
[seed 43] done: 10 rounds, 80 branches, 198 edges, 0 bugs (21.1s)
```

Dashboard is not supported in campaign mode (multiple Explorers would fight over the HTTP port). A future `--campaign-dashboard` could aggregate, but that's out of scope.

## Risks / Trade-offs

**[Memory pressure]** → Each seed's Explorer boots N VMs with 256 MB each. 5 seeds × 3 VMs × 256 MB = 3.8 GB. The 9950X3D has 128 GB. Manageable, but the user should be aware. Campaign mode logs total estimated memory at startup.

**[KVM FD limits]** → Each VM opens 1 VM FD + N vCPU FDs. 5 seeds × 3 VMs × 2 FDs = 30 FDs. Default `ulimit -n` is usually 1024+. Not a concern.

**[SIGALRM conflicts with SMP]** → Mitigated by forcing `num_workers: 1` and using `timer_create` with `SIGEV_THREAD_ID` per-thread in the Explorer's `run_bounded`. If the user requests SMP VMs (`--vcpus 2+`), campaign mode should work since each thread gets its own per-thread POSIX timer. The existing `init_thread_timers()` in WorkerPool already handles this. Campaign threads should call `init_thread_timers()` on the controller after bootstrap.

**[No cross-seed learning]** → Seeds are fully independent. If seed 42 finds an interesting fault schedule, seed 43 doesn't know. This is intentional — independence means reproducibility. Cross-seed sharing is future work (and would break per-seed determinism).

**[Report size]** → 100 seeds × full ExplorationReport could be large. Per-seed reports are written to disk, not held in memory. Campaign report only holds summaries + deduplicated bugs.
