## Why

The explorer has no structured performance observability. `perf_bench.rs` covers snapshot microbenchmarks, and `Instant` calls are scattered through the explorer and integration tests, but there's no way to answer "why is this exploration campaign slow" or detect performance regressions across commits. The hot path is KVM_RUN (opaque ioctl), so traditional CPU profiling gives limited insight — the useful metrics are exploration-specific: branches/sec, edges discovered/sec, snapshot overhead as a fraction of wall time.

## What Changes

- Structured exploration throughput metrics emitted as JSON lines during exploration runs, covering branches/sec, edges/sec, snapshot time, restore time, run time, and idle detection overhead.
- A `profiling` cargo feature on `chaoscontrol-explore` and `chaoscontrol-vmm` that enables `#[tracing::instrument]` on the ~10 hottest functions (snapshot, restore, run_bounded, step, handle_sdk_hypercall, coverage collection). Zero cost when disabled.
- A perf-stat wrapper script that runs `perf stat` against the VMM process during exploration, capturing IPC, cache misses, and branch mispredicts with no code changes required.
- Integration of throughput counters into `ExplorationReport` and `RoundHistory` so exploration output includes timing breakdowns without external tooling.

## Capabilities

### New Capabilities
- `exploration-throughput-metrics`: Structured per-round and per-branch timing counters (wall time breakdown for restore, run, snapshot, coverage collection) emitted as JSON lines and summarized in the exploration report.
- `tracing-instrumentation`: Opt-in `profiling` feature flag that adds `tracing::instrument` spans to hot VMM and explorer functions for use with `tokio-console`, `tracy`, or `perf` via `tracing-perfetto`.
- `perf-stat-script`: Shell script that wraps `perf stat -e cycles,instructions,cache-misses,branch-misses -p <pid>` for the VMM process during an exploration run.

### Modified Capabilities
- `exploration-timing`: Extend `RoundHistory` and `ExplorationReport` with per-phase wall-time breakdowns (restore_ms, run_ms, snapshot_ms, coverage_ms) and throughput rates.

## Impact

- **Crates**: `chaoscontrol-explore` (metrics collection, report format), `chaoscontrol-vmm` (tracing spans behind feature flag)
- **Dependencies**: `tracing` + `tracing-subscriber` as optional deps behind `profiling` feature. No new deps for the metrics path (uses `std::time::Instant`).
- **CLI**: New `--emit-metrics` flag on `chaoscontrol-explore run` to enable JSON-lines throughput output to stderr or a file.
- **Report format**: `report.txt` gains a "Performance" section; `RoundHistory` gains timing fields (backward-compatible via `#[serde(default)]`).
- **Scripts**: New `scripts/perf-stat.sh`.
- **No breaking changes.** All additions are additive or behind feature flags.
