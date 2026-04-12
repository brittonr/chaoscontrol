## Context

The explorer's inner loop is: restore snapshot → run VMs for N ticks → collect coverage → snapshot. Each phase has different cost characteristics. `run_bounded` is dominated by KVM_RUN ioctls (opaque), snapshot/restore cost scales with dirty pages (already benchmarked in `perf_bench.rs`), and coverage collection is a 64KB memcpy. Today there's a single `wall_clock_seconds` field on `RoundHistory` and `ExplorationReport`, but no per-phase breakdown and no machine-readable throughput stream.

The VMM functions (`run_bounded`, `snapshot`, `restore`, `handle_sdk_hypercall`) have no instrumentation points. Adding `tracing` spans behind a feature flag gives zero-cost opt-in profiling without affecting the default build.

## Goals / Non-Goals

**Goals:**
- Per-phase wall-time breakdown in every branch (restore, run, snapshot, coverage) so slow phases are immediately visible in reports.
- Machine-readable JSON-lines throughput stream for external consumption (dashboards, regression scripts, CI).
- Opt-in `tracing::instrument` spans on hot VMM/explorer functions for flamegraph-style profiling when investigating specific performance issues.
- `perf stat` wrapper script for quick IPC/cache-miss checks without code changes.

**Non-Goals:**
- Guest-side profiling. The guest runs under KVM — we can't instrument it from the host.
- Always-on tracing overhead. The `profiling` feature must be opt-in with zero cost when disabled.
- Custom profiling UI or dashboard. JSON-lines output is consumed by external tools.
- Benchmarking framework (criterion, etc.). The existing `perf_bench.rs` binary covers microbenchmarks.

## Decisions

### 1. Per-branch `BranchTimings` struct instead of per-round aggregation

Each branch execution records `restore_ms`, `run_ms`, `snapshot_ms`, `coverage_ms` as `f64` fields in a `BranchTimings` struct. `RoundHistory` aggregates these (sum/mean) rather than timing the round as a single blob. This lets us identify whether a slow round is slow because of one expensive branch (large dirty set) or uniformly slow (guest spinning).

Alternative: Time only at the round level. Rejected because branches within a round can vary — one branch hitting a ProcessKill fault triggers a VM reboot that dominates the round, and round-level timing hides which branch caused it.

### 2. JSON-lines to stderr via `--emit-metrics`

Throughput metrics are written as one JSON object per round to stderr (or a file via `--metrics-file`). Each line includes round number, branches, edges, bugs, and per-phase timing aggregates. Format: `{"round":1,"branches":4,"new_edges":12,"restore_ms":8.2,"run_ms":340.1,"snapshot_ms":15.3,"coverage_ms":0.4,"wall_ms":365.0}`.

stderr is chosen because stdout already carries the human-readable report in some modes. A separate `--metrics-file <path>` flag redirects to a file when stderr is noisy.

Alternative: Structured logging via `log` crate. Rejected because log lines are hard to parse reliably, and the existing `env_logger` setup mixes diagnostic output with metrics.

### 3. `profiling` feature flag with `tracing`

`chaoscontrol-vmm` and `chaoscontrol-explore` gain an optional `profiling` feature that pulls in `tracing` and annotates ~10 functions with `#[cfg_attr(feature = "profiling", tracing::instrument(skip_all))]`. The subscriber is configured by the binary, not the library — users choose `tracing-subscriber` with `fmt` layer, or `tracing-perfetto`, or `tracy-client`, whatever fits their toolchain.

Functions to instrument:
- `DeterministicVm::run_bounded`
- `DeterministicVm::snapshot` / `snapshot_incremental`
- `DeterministicVm::restore` / `restore_incremental`
- `DeterministicVm::handle_sdk_hypercall`
- `SimulationController::run`
- `SimulationController::snapshot_all` / `snapshot_all_incremental`
- `SimulationController::restore_all` / `restore_all_incremental`
- `Explorer::run_branch`

Alternative: Always-on tracing with runtime filtering. Rejected because even dormant `tracing` spans have non-zero cost (span creation, thread-local access) and the hot loop runs millions of iterations.

### 4. `perf stat` wrapper as a shell script

`scripts/perf-stat.sh` takes a `chaoscontrol-explore` command line, launches it in the background, attaches `perf stat` to the process, waits for completion, and prints the counters. No Rust code needed. Events: `cycles`, `instructions`, `cache-references`, `cache-misses`, `branch-instructions`, `branch-misses`, `context-switches`.

Alternative: Embed perf counter reading via `perf_event_open` in Rust. Rejected because it adds complexity and platform-specific code for something that's used quarterly at most. The napkin already documents that PMU on AMD Zen5 has quirks — a shell script is easier to adjust.

### 5. Extend `RoundHistory` and report, not replace them

New timing fields are added to the existing `RoundHistory` struct with `#[serde(default)]`. The `ExplorationReport` gains a "Performance" section in `format_report()`. No structural changes to checkpointing — timing fields serialize naturally alongside existing fields.

## Risks / Trade-offs

- **[Timing overhead]** → `Instant::now()` calls around each phase add ~50ns each. With 4 calls per branch and 16 branches per round, that's ~3.2µs per round — negligible against multi-second branch execution.
- **[stderr pollution]** → `--emit-metrics` writes to stderr by default, mixing with `env_logger` output. Mitigation: `--metrics-file` flag, and metrics lines are valid JSON (easy to grep/filter).
- **[Feature flag maintenance]** → `profiling` feature adds conditional compilation. Mitigation: CI runs `cargo check --features profiling` to catch bitrot. The `cfg_attr` pattern means zero code duplication.
- **[tracing version churn]** → `tracing` is a widely-used crate but has had breaking changes. Mitigation: pin to `tracing = "0.1"` (stable series), feature-gated so it doesn't affect default builds.
