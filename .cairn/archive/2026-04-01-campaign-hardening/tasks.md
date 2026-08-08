## 1. Graceful Shutdown

- [x] 1.1 Add `static SHUTDOWN: AtomicBool` and `static SIGNAL_COUNT: AtomicU32` globals in a new `crates/chaoscontrol-explore/src/signal.rs` module
- [x] 1.2 Implement `install_signal_handlers()` using `libc::sigaction` for SIGINT and SIGTERM — first signal sets flag, second signal calls `std::process::exit(1)`
- [x] 1.3 Call `install_signal_handlers()` at the top of `main()` in `chaoscontrol-explore` CLI binary
- [x] 1.4 Add shutdown flag check in `Explorer::run()` after each round — save checkpoint, emit `Finished { reason: "interrupted" }`, break loop
- [x] 1.5 Add shutdown flag check in `CampaignRunner::run()` after each seed completes — skip remaining seeds, save progress, aggregate partial results
- [x] 1.6 Add `"interrupted"` to the set of `finish_reason` values handled by the dashboard event receiver
- [x] 1.7 Test: unit test for `install_signal_handlers` (verify flag is initially false, verify double-install is safe)
- [x] 1.8 Test: Explorer returns partial results when shutdown flag is manually set (set flag before round 3 of 10, verify report has 2 rounds)

## 2. Panic Isolation

- [x] 2.1 Replace `.join().unwrap()` in `CampaignRunner::run()` (campaign.rs:229) with `catch_unwind` inside the thread closure — return `Result<SeedResult, String>` capturing the panic message
- [x] 2.2 Add `SeedResult::Failed { seed: u64, error: String }` variant or equivalent — campaign runner collects failures alongside successes
- [x] 2.3 Update `CampaignProgress` to track failed seeds: add `failed: BTreeMap<u64, String>` field with `#[serde(default)]`
- [x] 2.4 Update `CampaignReport` to include `failed_seeds: Vec<(u64, String)>` and `seeds_failed` count
- [x] 2.5 Update `format_campaign_report()` to show failed seeds in the report header and per-seed table
- [x] 2.6 Replace `.join().unwrap()` in `WorkerPool::new()` (worker.rs:104) with `catch_unwind` — create pool with surviving workers, return error if all fail
- [x] 2.7 Replace `.join().unwrap()` in `WorkerPool::run_branches()` (worker.rs:217) with `catch_unwind` inside the branch loop — panicked branches return zero-coverage placeholder `BranchResult`
- [x] 2.8 Log panics at `error` level with seed/worker/branch context and panic message
- [x] 2.9 Test: campaign with one poisoned seed (inject panic via a custom ExplorerConfig that causes immediate failure) — verify other seeds complete and report aggregates correctly
- [x] 2.10 Test: worker pool branch panic — verify round continues and zero-coverage result is returned

## 3. Exploration Timing

- [x] 3.1 Add `wall_clock_seconds: f64` to `ExplorationReport` struct
- [x] 3.2 Add `Instant::now()` at top of `Explorer::run()`, compute elapsed at return, set `wall_clock_seconds`
- [x] 3.3 Add `wall_clock_seconds: f64` to `RoundHistory` struct with `#[serde(default)]` for backward compat
- [x] 3.4 Wrap each round's `explore_round()` / `explore_input_tree_round()` call in per-round timing, store in `RoundHistory`
- [x] 3.5 Add `wall_clock_seconds` field to `DashboardEvent::RoundComplete` with `#[serde(default)]`
- [x] 3.6 Update `format_report()` to show wall-clock time in summary and time column in round history table (show "—" for 0.0 values)
- [x] 3.7 Update `CheckpointConfig` or `ExplorationCheckpoint` if timing needs to survive resume (round_history already serialized, just needs the new field)
- [x] 3.8 Test: verify `ExplorationReport.wall_clock_seconds > 0.0` after a simple exploration run
- [x] 3.9 Test: verify `RoundHistory` serialization roundtrip with and without `wall_clock_seconds`

## 4. CLI Completeness

- [x] 4.1 Add `--stale-round-limit <N>` flag to `Commands::Run` with default 10
- [x] 4.2 Add `--stale-round-limit <N>` flag to `Commands::Campaign` with default 10
- [x] 4.3 Wire both flags through to `ExplorerConfig.stale_round_limit`
- [x] 4.4 Add `--workers-per-seed <N>` flag to `Commands::Campaign` with default 0 (auto)
- [x] 4.5 Implement auto-compute logic: `max(1, available_cores / (num_seeds * num_vms))` when workers-per-seed is 0
- [x] 4.6 Change campaign mode to use `workers-per-seed` value instead of hardcoded 1 for `ExplorerConfig.num_workers`
- [x] 4.7 If `--workers` is passed alongside `--campaign-seeds`, log a warning suggesting `--workers-per-seed`
- [x] 4.8 Test: verify `--stale-round-limit 0` disables early stopping (mock or short run)
- [x] 4.9 Test: verify `--workers-per-seed` auto-compute produces sensible values (unit test the formula)

## 5. Memory Guard

- [x] 5.1 Add `read_available_memory_mb() -> Option<usize>` function that parses `MemAvailable` from `/proc/meminfo`
- [x] 5.2 Add `check_memory(estimated_mb: usize, strict: bool) -> Result<(), String>` that warns or errors if over 80% threshold
- [x] 5.3 Call `check_memory()` at the start of `cmd_run()` and `cmd_campaign()` in the CLI binary
- [x] 5.4 Add `--strict-memory` flag to `Commands::Run` and `Commands::Campaign`
- [x] 5.5 Log memory estimate and available memory at `info` level regardless of threshold
- [x] 5.6 Test: `read_available_memory_mb()` returns `Some(n)` where n > 0 on Linux
- [x] 5.7 Test: `check_memory()` returns Ok when estimate is well under threshold, Err when over in strict mode

## 6. Auto-Minimize

- [x] 6.1 Add `--auto-minimize` flag to `Commands::Run` and `Commands::Campaign`
- [x] 6.2 Implement `auto_minimize_bugs()` function that takes a list of `SerializableBug`, base `ExplorerConfig`, output dir — runs minimizer on each bug sequentially
- [x] 6.3 Check shutdown flag before each bug minimization — skip remaining if interrupted
- [x] 6.4 Save minimized results as `bug_N_min.json`; log per-bug progress (original faults → minimized faults, time)
- [x] 6.5 Call `auto_minimize_bugs()` after `Explorer::run()` returns in `cmd_run()` when flag is set
- [x] 6.6 Call `auto_minimize_bugs()` after `CampaignRunner::run()` returns in `cmd_campaign()` when flag is set — use deduplicated campaign bugs
- [x] 6.7 Handle minimization failure gracefully (bug no longer reproduces) — warn and keep original
- [x] 6.8 Test: auto-minimize with a bug that has 0 faults (edge case — already minimal, should no-op)

## 7. Integration & Cleanup

- [x] 7.1 Run `cargo clippy --all-targets -- -D warnings` and fix any new warnings
- [x] 7.2 Run `cargo fmt --all`
- [x] 7.3 Run full test suite (`cargo test`) — verify all existing tests still pass
- [x] 7.4 Update `format_campaign_report()` to include timing, failed seeds, and minimization summary sections
- [x] 7.5 Update the `explore-raft` and `explore-redb` nix wrappers if any default flag values changed
- [x] 7.6 Verify `nix flake check` passes (build + clippy + fmt + tests)
