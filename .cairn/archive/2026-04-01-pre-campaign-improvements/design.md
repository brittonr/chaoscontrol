## Context

ChaosControl has a working exploration engine, campaign runner, live dashboard, redb crash-consistency guest, and fault minimizer. The system can run multi-seed campaigns that launch N independent explorations in parallel, each with its own KVM VMs. However, several gaps exist that would cause friction or data loss during production campaigns:

1. The redb guest has 13 clippy warnings that block `cargo clippy -D warnings`
2. Campaign results are only printed to stdout — not persisted to disk
3. A crashed campaign loses all progress (no incremental checkpoint)
4. The dashboard doesn't work in campaign mode (no events emitted)
5. `ProcessRestart` sets a status flag but doesn't actually reboot the VM
6. The `explore-redb` nix wrapper uses distributed-system defaults (3 VMs) instead of storage-appropriate ones (1 VM)

## Goals / Non-Goals

**Goals:**
- Fix clippy warnings so CI passes cleanly
- Persist campaign reports and progress incrementally
- Support campaign resume after crash
- Wire dashboard into campaign mode with seed-attributed events
- Implement actual VM reboot for ProcessRestart (kernel reload, disk preservation)
- Tune nix wrappers for redb workload characteristics

**Non-Goals:**
- Rewriting the campaign runner's parallelism model (scoped threads work fine)
- Adding a campaign-specific dashboard UI page (existing UI can show seed-attributed events)
- Supporting ProcessRestart for SMP VMs (single-vCPU restart is sufficient for now)
- Making the redb guest multi-VM (it tests a single-node embedded database)

## Decisions

### D1: Clippy fixes are mechanical
Apply `c"..."` nul-terminated string literals, remove unnecessary casts, use `.flatten()` on iterators. No behavioral changes.

**Alternatives**: Suppress warnings with `#[allow(...)]` — rejected because these are real code quality improvements with no risk.

### D2: Campaign report persistence in CampaignRunner::run()
Save `campaign_report.json` and `campaign_report.txt` at the end of `CampaignRunner::run()`, right after `aggregate_reports()`. The output directory already exists (created early in `run()`).

**Alternatives**: Save in `cmd_campaign()` in the CLI binary — rejected because the runner owns the output directory path and report data.

### D3: Incremental checkpoint via CampaignProgress struct
New struct `CampaignProgress` serialized to `campaign_progress.json`:
```rust
struct CampaignProgress {
    seeds: Vec<u64>,              // all seeds in the campaign
    config: SerializableExplorerConfig,  // base config (minus non-serializable fields)
    completed: BTreeMap<u64, SeedSummary>,  // seed → summary
}
```

Written after each seed completes, inside the `seed_results` collection loop. Resume reads it, builds `remaining_seeds = seeds - completed.keys()`, runs only those.

**Alternatives**: 
- Use per-seed checkpoint.json files only — harder to detect which seeds were part of the original campaign vs. unrelated runs.
- Save full ExplorationReport per seed — too large; SeedSummary is enough for aggregation.

### D4: Campaign resume as CLI subcommand
`chaoscontrol-explore campaign resume --corpus <dir> [--rounds N]`. Reads `campaign_progress.json`, reconstructs the `ExplorerConfig`, runs remaining seeds. Merges completed seed summaries from checkpoint with new seed results before calling `aggregate_reports()`.

The `--rounds` flag allows extending exploration depth on resume (e.g., crashed at round 50, resume with 200 rounds total).

### D5: Dashboard wiring via shared SyncSender
Campaign mode calls `server::start(port)` once before launching seeds. The returned `SyncSender<DashboardEvent>` is wrapped in `Arc` and cloned into each seed's `Explorer`. Each Explorer already has `Option<SyncSender<DashboardEvent>>` — just set it.

Campaign-level events (`campaign_started`, `seed_complete`, `campaign_finished`) are sent directly from `CampaignRunner::run()` before/after the scoped thread block.

Seed attribution: extend `DashboardEvent` variants with an `Option<u64>` `seed` field. Single-run mode sets it to `None`, campaign mode sets it to `Some(seed)`.

### D6: ProcessRestart via kernel reload
The controller already has `schedule_restart()` which sets `VmStatus::Restarting { restart_at_tick }`. The missing piece is in `step_round()` — when a VM's status is `Restarting` and the tick has arrived, actually perform the reboot:

1. Reset CPU registers to boot entry (reuse `DeterministicVm::setup_boot_params()`)
2. Reload kernel + initrd into guest memory (reuse `load_kernel()`)
3. Clear coverage bitmap
4. Reset fault engine state (but preserve disk dirty pages)
5. Re-initialize virtio device queues (net and rng get fresh state, block keeps CoW overlay)
6. Run until `setup_complete` or bootstrap budget exhausted

The key insight: `DeterministicBlock`'s CoW overlay (`dirty: BTreeMap<usize, Vec<u8>>`) is separate from the base image. During restart, we preserve the dirty map while reinitializing everything else. This means a `DeterministicVm` needs a `restart()` method that does `load_kernel()` + `setup_boot_params()` without reconstructing the block device.

### D7: Explore-redb wrapper defaults
Change flake.nix `explore-redb` wrapper:
- `--vms 1` (single-node storage, not distributed)
- `--ticks 5000` (long enough for crash/recovery cycles)
- `--rounds 100`
- `--branches 8`
- `--mode hybrid`
- Keep `--disk-image ${redb-disk-image}`

Verify `redb-disk-image` is at least 64 MB. Current derivation uses `pkgs.runCommand` to create an ext4 image — check `count=` parameter.

## Risks / Trade-offs

- **[ProcessRestart complexity]** → Kernel reload touches low-level VM setup code. Mitigated by reusing existing `load_kernel()` and `setup_boot_params()` paths. Integration test required to verify determinism across restart.

- **[Campaign checkpoint race]** → Multiple seeds complete near-simultaneously, concurrent writes to `campaign_progress.json`. Mitigated by writing inside the sequential `seed_results` collection loop (results are joined one at a time), not inside the parallel threads.

- **[Dashboard event backpressure]** → 5+ seeds emitting events simultaneously through a single `SyncSender` with buffer size 64. Mitigated by using `try_send()` and dropping events on backpressure (dashboard is best-effort, not data-critical). Client can always poll `/api/state` for authoritative snapshot.

- **[Disk preservation across restart]** → The CoW block device's dirty pages must survive the restart but virtio queue state must not. Need to split the device reinitialization carefully. Mitigated by the existing `DeterministicBlock` API which separates data (base + dirty) from transport (virtio queue state).
