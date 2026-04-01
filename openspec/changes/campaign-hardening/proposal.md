## Why

Long-running campaigns (overnight, multi-seed) lose data and crash in preventable ways. Ctrl-C kills the process with no checkpoint save. A thread panic in one seed takes down the entire campaign. There's no wall-clock timing in reports, no memory safety net, and the stale-round limit isn't configurable from the CLI. Bugs pile up un-minimized. These gaps make campaigns fragile enough that you have to babysit them.

## What Changes

- Install a signal handler (SIGINT/SIGTERM) that sets an atomic flag, checked after each round. On signal: save checkpoint, emit `Finished { reason: "interrupted" }`, exit cleanly.
- Replace `.join().unwrap()` with panic-catching `match` in campaign seed threads and worker pool branch threads. Log the failure, mark the seed/branch as failed, continue with remaining work.
- Add `wall_clock_seconds: f64` to `ExplorationReport` and `RoundHistory`. Measure with `Instant::now()` in `Explorer::run()` and per-round.
- Expose `--stale-round-limit` on both `Run` and `Campaign` CLI subcommands (currently hardcoded to 10).
- Check `/proc/meminfo` MemAvailable against the memory estimate at campaign startup. Warn (or refuse with `--strict-memory`) if estimate exceeds 80% of available.
- Allow `--workers-per-seed` in campaign mode instead of silently forcing 1, with auto-compute default: `max(1, cores / (seeds × vms))`.
- Add `--auto-minimize` flag that runs the minimizer on each bug after exploration finishes.

## Capabilities

### New Capabilities
- `graceful-shutdown`: Signal handling for clean Ctrl-C / SIGTERM shutdown with checkpoint persistence
- `panic-isolation`: Thread panic recovery in campaign seeds and worker pool branches without process abort
- `exploration-timing`: Wall-clock timing in ExplorationReport and per-round RoundHistory
- `memory-guard`: Pre-flight memory availability check against campaign VM memory estimate
- `auto-minimize`: Post-campaign automatic bug schedule minimization

### Modified Capabilities
- `campaign-runner`: Expose stale-round-limit CLI flag, workers-per-seed tuning, panic isolation for seed threads
- `parallel-exploration`: Panic isolation for worker pool branch threads

## Impact

- `crates/chaoscontrol-explore/src/explorer.rs`: Signal flag check in main loop, timing instrumentation, report struct changes
- `crates/chaoscontrol-explore/src/campaign.rs`: Panic recovery for seed threads, memory guard, workers-per-seed logic
- `crates/chaoscontrol-explore/src/worker.rs`: Panic recovery for branch threads
- `crates/chaoscontrol-explore/src/bin/chaoscontrol-explore.rs`: New CLI flags (--stale-round-limit, --workers-per-seed, --auto-minimize, --strict-memory)
- `crates/chaoscontrol-explore/src/report.rs`: Wall-clock formatting
- `crates/chaoscontrol-explore/src/dashboard_types.rs`: Timing fields in events
- Adds `ctrlc` crate dependency (or raw `libc::signal`)
