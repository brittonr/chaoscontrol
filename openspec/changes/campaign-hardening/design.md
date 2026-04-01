## Context

The exploration engine and campaign runner work for supervised short runs but break down for unattended multi-hour campaigns. The current failure modes:

1. **Ctrl-C = data loss.** No signal handling. Process dies, partial checkpoint from last completed round survives (if output dir was set), but the current round's work and any standalone bugs are gone. Campaign mode only writes `campaign_progress.json` after each seed completes — Ctrl-C mid-seed loses the entire seed.

2. **Thread panics = total loss.** Three `.join().unwrap()` sites propagate panics: campaign seed threads (`campaign.rs:229`), worker bootstrap threads (`worker.rs:104`), and worker branch threads (`worker.rs:217`). A KVM EINVAL, snapshot corruption, or OOM in one thread kills everything.

3. **No timing data.** `ExplorationReport` has no `wall_clock_seconds`. `RoundHistory` has no per-round timing. The campaign report has timing, but single-seed `run` output doesn't. Can't tell if a 50-round run took 5 minutes or 5 hours.

4. **Stale-round limit hardcoded.** `stale_round_limit: 10` is set in the CLI config builders but not exposed as a flag. The Raft guest with 3 nodes plateaus after round 3; running 10 stale rounds wastes 7× the time.

5. **No memory check.** 8 seeds × 3 VMs × 256 MB = 6 GB. Plus frontier snapshots (each full snapshot is ~256 MB per VM). Easy to OOM on a 16 GB machine.

6. **Campaign forces workers=1.** On a 32-core machine with 4 seeds × 3 VMs = 12 cores used, 20 idle. Each seed runs branches sequentially.

7. **Bugs un-minimized.** The minimizer exists but requires a manual `minimize` subcommand per bug. Overnight campaigns can find 20+ bugs that all need separate minimize runs.

## Goals / Non-Goals

**Goals:**
- Unattended overnight campaigns that survive Ctrl-C, thread panics, and memory pressure without losing results
- Timing instrumentation for performance analysis and regression detection
- CLI completeness: all tuning knobs accessible without code changes
- Automatic bug minimization as an opt-in post-processing step

**Non-Goals:**
- Distributed campaigns across multiple machines (single-machine, multi-thread only)
- Live campaign modification (changing parameters mid-run)
- Automatic parameter tuning (adaptive stale limits, branch counts, etc.)
- Per-round checkpoint in campaign mode (only per-seed granularity; within-seed checkpoints already exist)

## Decisions

### 1. Signal handling: `AtomicBool` flag, not `ctrlc` crate

Use raw `libc::sigaction` to install a handler for SIGINT and SIGTERM that sets `static SHUTDOWN: AtomicBool`. The explorer checks `SHUTDOWN.load(Relaxed)` after each round. The campaign runner checks it between seed launches.

**Why not `ctrlc` crate:** It pulls in nix/libc transitively and installs a global handler that conflicts with the SIGALRM handler already used for SMP preemption. Raw `sigaction` is 10 lines and gives full control over signal mask.

**Why `Relaxed` ordering:** The flag is written by signal handler (one store) and read by one thread (polling). No data dependencies — `Relaxed` is sufficient.

**Behavior on signal:**
- Explorer: finish current branch (< 5 seconds), save checkpoint, emit `Finished { reason: "interrupted" }`, return report.
- Campaign: after current round completes, save `campaign_progress.json`, skip remaining seeds, aggregate partial results, save partial campaign report.
- Second signal: `std::process::exit(1)` — force kill if stuck.

### 2. Panic isolation: `catch_unwind` at thread boundaries

Wrap each seed's `Explorer::new().run()` in `std::panic::catch_unwind(AssertUnwindSafe(|| ...))`. On panic, log the backtrace via `std::panic::set_hook`, record `SeedResult::Failed { seed, error: String }` in campaign progress, continue with remaining seeds.

Same for `WorkerPool::run_branches`: wrap each worker's branch execution in `catch_unwind`. On panic, return a `BranchResult` placeholder with zero coverage and an error flag. The explorer treats it as a branch that found nothing — no coverage, no bugs, no frontier entry.

**Why `AssertUnwindSafe`:** The `SimulationController` contains KVM file descriptors and `Arc<Vec<u8>>` memory bases. These are unwind-safe in practice (no interior mutability that could be corrupted), but don't implement `UnwindSafe`. The wrapper is a deliberate opt-in.

**Why not `thread::Builder::spawn` with error return:** We already use `std::thread::scope` which requires `join().unwrap()` for the scope to return. The `catch_unwind` goes inside the closure, before the result is returned.

### 3. Timing: `Instant::now()` in `Explorer::run()` and per-round

Add `start: Instant` at the top of `Explorer::run()`. Compute `elapsed` at the end. Store in `ExplorationReport::wall_clock_seconds`.

Per-round: wrap the `explore_round()` / `explore_input_tree_round()` call in timing. Store in `RoundHistory::wall_clock_seconds`.

Dashboard event `RoundComplete` gets a `wall_clock_seconds` field. Backward-compatible: `#[serde(default)]` on the new field.

### 4. Memory guard: read `/proc/meminfo`, warn or refuse

At campaign startup (and single-seed `run` startup), compute:
```
estimated_mb = num_seeds * num_vms * vm_memory_mb
             + num_seeds * max_frontier * num_vms * vm_memory_mb  // frontier snapshots worst-case
```

Read `MemAvailable` from `/proc/meminfo`. If `estimated_mb > 0.8 * available_mb`:
- Default: print warning to stderr, continue
- `--strict-memory`: exit with error

Frontier snapshot estimate is pessimistic (incremental snapshots are much smaller), but better to over-warn than OOM.

### 5. Workers-per-seed: auto-compute with override

Default: `max(1, available_cores / (num_seeds * num_vms))`. Minimum 1.

CLI: `--workers-per-seed N` on the `campaign` subcommand. 0 = auto (default).

The existing `--workers` flag on `run` is unrelated (it controls parallelism for a single-seed run). Campaign mode currently forces `num_workers = 1`. Change it to use the auto-computed or user-specified value.

**Constraint:** Multi-vCPU VMs with SIGALRM preemption need per-thread timers (`timer_create` + `SIGEV_THREAD_ID`). Single-vCPU VMs (the common case) don't arm the timer, so parallel workers are safe. The worker pool already documents this.

### 6. Auto-minimize: post-campaign pass

After `CampaignRunner::run()` returns, if `--auto-minimize` is set, iterate over `campaign_report.bugs`. For each bug, run `Minimizer::new(config, bug).minimize()`. Save as `bug_N_min.json` alongside the original.

Sequential, not parallel — minimization itself runs branches and needs KVM. Running minimizers in parallel with the campaign's VMs would double memory pressure. Run them after all seeds are done and VMs are dropped.

Respect the shutdown flag: if SIGINT arrived during the campaign, skip minimization.

### 7. Stale-round-limit CLI flag

Add `--stale-round-limit <N>` to both `Run` and `Campaign` subcommands. Default: 10 (matches current hardcoded value). 0 = disable early stopping.

## Risks / Trade-offs

- **[Signal handler races]** → The SIGALRM handler for SMP preemption and the new SIGINT/SIGTERM handler coexist. SIGALRM uses `SA_RESTART`=false to interrupt `vcpu.run()`. SIGINT handler just sets a flag — no restart interaction. The signal mask in the handler blocks SIGALRM during SIGINT handling to avoid re-entrancy.
- **[catch_unwind doesn't catch all aborts]** → `std::process::abort()`, stack overflow, and double panics bypass `catch_unwind`. KVM ioctls that segfault will still kill the process. This is acceptable — those are bugs in the VMM, not in the guest under test.
- **[Memory estimate is coarse]** → Frontier snapshots use incremental CoW and are much smaller than full VM memory. The estimate over-counts. False warnings are better than silent OOMs, and `--strict-memory` is opt-in.
- **[Auto-minimize wall-clock]** → Minimization runs ddmin which is O(n log n) branches per bug. 20 bugs × 50 faults × log(50) rounds ≈ ~6000 branch executions. At ~0.5s per branch, that's ~50 minutes. Acceptable as a background post-processing step, but should be clearly logged.
- **[Workers-per-seed contention]** → Multiple workers within a seed share a `SimulationSnapshot` via `Arc`. The snapshot is read-only during branch execution. No contention on the hot path. Memory overhead is the additional `SimulationController` instances (one per worker), each with their own KVM VMs.
