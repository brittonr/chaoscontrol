## 1. Clippy Fixes (redb guest)

- [x] 1.1 Replace `b"devtmpfs\0".as_ptr().cast()` with `c"devtmpfs".as_ptr()` (and all other nul-terminated string literals) in `chaoscontrol-redb-guest/src/main.rs`
- [x] 1.2 Remove unnecessary `usize` casts and use `.is_multiple_of()` where flagged
- [x] 1.3 Replace `if let Ok(pair) = entry` in iterator with `.flatten()` in `rebuild_oracle_from_db`
- [x] 1.4 Verify `cargo clippy --all-targets -- -D warnings` passes clean

## 2. Campaign Report Persistence

- [x] 2.1 In `CampaignRunner::run()`, after `aggregate_reports()`, write `campaign_report.json` (serde_json::to_string_pretty) and `campaign_report.txt` (format_campaign_report) to `self.config.output_dir`
- [x] 2.2 Add test: `campaign_report_files_written` — run aggregate_reports with mock data, write to tempdir, verify both files exist and deserialize/contain expected content

## 3. Campaign Incremental Checkpoint

- [x] 3.1 Define `CampaignProgress` struct in `campaign.rs` with fields: `seeds: Vec<u64>`, `config: SerializableCampaignConfig`, `completed: BTreeMap<u64, SeedSummary>` — derive Serialize/Deserialize
- [x] 3.2 Define `SerializableCampaignConfig` with the serializable subset of `ExplorerConfig` (kernel_path, initrd_path, num_vms, branch_factor, ticks_per_branch, max_rounds, quantum, exploration_mode, seed, disk_image_path, bootstrap_budget, stale_round_limit)
- [x] 3.3 In `CampaignRunner::run()`, after collecting each seed result in the sequential join loop, write updated `campaign_progress.json` to output_dir
- [x] 3.4 Add `save_campaign_progress()` and `load_campaign_progress()` functions
- [x] 3.5 Add serde roundtrip test for `CampaignProgress`

## 4. Campaign Resume

- [x] 4.1 Add `campaign resume --corpus <dir>` subcommand to CLI binary — reads `campaign_progress.json`, computes remaining seeds
- [x] 4.2 Implement `CampaignRunner::resume(progress: CampaignProgress)` that runs only remaining seeds, merges results with completed summaries
- [x] 4.3 In resume path, reconstruct `ExplorerConfig` from `SerializableCampaignConfig` fields
- [x] 4.4 After resume completes all remaining seeds, write final `campaign_report.json` and `campaign_report.txt`
- [x] 4.5 Add test: resume with all seeds complete → no exploration launched, report written

## 5. Campaign Dashboard Integration

- [x] 5.1 Add `seed: Option<u64>` field to all `DashboardEvent` variants (with `#[serde(skip_serializing_if = "Option::is_none")]`)
- [x] 5.2 Add `DashboardEvent::CampaignStarted`, `DashboardEvent::SeedComplete`, `DashboardEvent::CampaignFinished` variants
- [x] 5.3 Add campaign-level fields to `DashboardState`: `mode: String`, `seeds_total: usize`, `seeds_completed: usize`, `seed_summaries: Vec<SeedSummary>`
- [x] 5.4 In `cmd_campaign()`, accept `--dashboard` and `--dashboard-port` flags; call `server::start(port)` before launching seeds
- [x] 5.5 Pass the `SyncSender<DashboardEvent>` into `CampaignRunner` and forward it to each seed's `Explorer`
- [x] 5.6 Emit `CampaignStarted` before scoped thread block, `SeedComplete` after each seed joins, `CampaignFinished` after aggregation
- [x] 5.7 Update `event_receiver_loop` to handle new campaign event variants and update `DashboardState` campaign fields
- [x] 5.8 Update SSE handler to map new event types to SSE event names (`campaign_started`, `seed_complete`, `campaign_finished`)

## 6. ProcessRestart VM Reboot

- [x] 6.1 Add `DeterministicVm::restart()` method that reloads kernel/initrd into guest memory, resets CPU registers to boot entry, clears coverage bitmap, but preserves the `DeterministicBlock` CoW dirty pages
- [x] 6.2 Add `DeterministicBlock::preserve_for_restart()` or equivalent that saves the dirty overlay, allowing reinitialization of virtio transport state while keeping data
- [x] 6.3 In `SimulationController::step_round()`, when a VM has `VmStatus::Restarting` and `tick >= restart_at_tick`, call `vm.restart()` followed by `run_until_setup_complete()` with the bootstrap budget
- [x] 6.4 On successful restart, set VM status to `Running`; on budget exceeded, set to `Crashed`
- [x] 6.5 Add unit test: `DeterministicBlock` dirty pages survive restart (write data, restart, read back)
- [x] 6.6 Add integration test: ProcessKill + ProcessRestart fault schedule — VM reboots, guest sees persistent data on disk, assertions pass

## 7. Explore-Redb Wrapper Tuning

- [x] 7.1 Update `explore-redb` wrapper in `flake.nix`: change `--vms 3` to `--vms 1`, add `--ticks 5000`
- [x] 7.2 Check `redb-disk-image` derivation size — ensure at least 64 MB; increase `count=` parameter if needed
- [x] 7.3 Update `redb-sim` mkChaosTest to use `vms = 1` and `ticks = 5000`
- [x] 7.4 Verify `nix build .#initrd-redb` and `nix build .#redb-disk-image` build successfully

## 8. Final Verification

- [x] 8.1 Run `cargo clippy --all-targets -- -D warnings` — zero warnings
- [x] 8.2 Run `cargo test` — all tests pass
- [x] 8.3 Run `cargo fmt --all --check` — no formatting issues
- [x] 8.4 Verify `nix flake check` passes (build + clippy + fmt + tests)
