## Why

The exploration engine, campaign runner, redb guest, and dashboard are all built, but several gaps remain that would cause friction or lost work during real multi-seed campaigns. Clippy failures block CI, campaign results vanish on crash, the dashboard goes dark in campaign mode, and ProcessRestart doesn't actually reboot the VM — making crash-recovery testing incomplete.

## What Changes

- Fix 13 clippy warnings in `chaoscontrol-redb-guest` (nul-terminated strings, unnecessary casts, `manual_flatten`)
- Save `campaign_report.json` and `campaign_report.txt` to the output directory after a campaign completes
- Add incremental campaign checkpoint that tracks completed seeds, enabling resume after crash
- Wire the live dashboard into campaign mode so multi-seed runs are observable
- Implement actual VM reboot for `ProcessRestart` fault — reload kernel/initrd and run until `setup_complete`
- Verify and tune `explore-redb` nix wrapper defaults (single VM, adequate ticks, disk image sizing)

## Capabilities

### New Capabilities
- `campaign-persistence`: Save campaign reports to disk and support incremental campaign checkpoint/resume across seeds
- `campaign-dashboard`: Dashboard support for multi-seed campaign mode with per-seed and aggregate views
- `process-restart-reboot`: ProcessRestart fault actually reboots a VM through kernel reload and re-bootstrap

### Modified Capabilities
- `campaign-runner`: Add checkpoint/resume support and report persistence
- `dashboard-server`: Extend SSE events and state model for multi-seed campaign aggregation
- `nix-test-runner`: Tune explore-redb wrapper defaults for storage workloads

## Impact

- **Crates affected**: `chaoscontrol-redb-guest`, `chaoscontrol-explore` (campaign, server, dashboard_types, explorer, bin), `chaoscontrol-vmm` (controller), `chaoscontrol-dashboard`
- **CLI**: New `campaign resume` subcommand, `--dashboard` flag on `campaign` subcommand
- **Nix**: Updated `explore-redb` wrapper defaults in `flake.nix`
- **No breaking changes** — all additions are backward-compatible
