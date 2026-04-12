## 1. Per-branch timing infrastructure

- [ ] 1.1 Add `BranchTimings` struct to `chaoscontrol-explore/src/explorer.rs` with `restore_ms`, `run_ms`, `snapshot_ms`, `coverage_ms` fields (all `f64`)
- [ ] 1.2 Instrument `Explorer::run_branch` to record `Instant` around each phase (restore, run, snapshot, coverage) and populate `BranchTimings`
- [ ] 1.3 Add `restore_ms`, `run_ms`, `snapshot_ms`, `coverage_ms` fields to `RoundHistory` with `#[serde(default)]`
- [ ] 1.4 Aggregate per-branch timings into `RoundHistory` after each round completes (sum across branches)
- [ ] 1.5 Add unit tests: `BranchTimings` default, `RoundHistory` serde roundtrip with and without timing fields, old checkpoint backward compat

## 2. Throughput metrics in ExplorationReport

- [ ] 2.1 Add `branches_per_second` and `edges_per_second` fields (`f64`) to `ExplorationReport`
- [ ] 2.2 Compute throughput at report construction: `total_branches / wall_clock_seconds` (0.0 when wall time is 0.0)
- [ ] 2.3 Add "Performance" section to `format_report()` showing wall time, throughput rates, and per-phase breakdown (percentage of wall time in restore/run/snapshot/coverage)
- [ ] 2.4 Extend round history table in `format_report()` with per-phase columns when any round has non-zero timing
- [ ] 2.5 Add unit tests: throughput computation, zero wall-time guard, report formatting with and without timing data

## 3. JSON-lines metrics output

- [ ] 3.1 Add `--emit-metrics` flag and `--metrics-file <path>` option to `chaoscontrol-explore run` CLI args
- [ ] 3.2 Define `MetricsLine` struct (serde Serialize) with `round`, `branches`, `new_edges`, `cumulative_edges`, `bugs_found`, `restore_ms`, `run_ms`, `snapshot_ms`, `coverage_ms`, `wall_ms`
- [ ] 3.3 Wire metrics emission into the explorer's round-completion path: serialize `MetricsLine` to the configured output (stderr or file)
- [ ] 3.4 Add unit test: `MetricsLine` serializes to valid single-line JSON with all expected fields

## 4. Tracing instrumentation feature flag

- [ ] 4.1 Add `profiling = ["tracing"]` feature to `chaoscontrol-vmm/Cargo.toml` and `chaoscontrol-explore/Cargo.toml` with `tracing = { version = "0.1", optional = true }`
- [ ] 4.2 Annotate the 12 target functions with `#[cfg_attr(feature = "profiling", tracing::instrument(skip_all))]`
- [ ] 4.3 Verify `cargo check -p chaoscontrol-vmm` and `cargo check -p chaoscontrol-vmm --features profiling` both compile
- [ ] 4.4 Add `cargo check --features profiling` to the nix flake check or CI script to prevent bitrot

## 5. perf-stat wrapper script

- [ ] 5.1 Write `scripts/perf-stat.sh` that launches a command in the background, attaches `perf stat -p <pid>` with default counters, waits, and prints results
- [ ] 5.2 Support `PERF_EVENTS` env var to override default counter list
- [ ] 5.3 Handle error cases: command fails to start, `perf` not available, process exits before attach
- [ ] 5.4 Add usage comment and make script executable

## 6. Integration and validation

- [ ] 6.1 Run `cargo test -p chaoscontrol-explore` — all existing + new tests pass
- [ ] 6.2 Run `cargo clippy --all-targets -- -D warnings` clean
- [ ] 6.3 Run `cargo fmt --all --check` clean
- [ ] 6.4 Verify checkpoint backward compat: load an existing checkpoint JSON (without new timing fields) and confirm it deserializes without error
