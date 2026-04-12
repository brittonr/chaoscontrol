## Why

ChaosControl can already inject disk, process, network, and scheduling faults, but it does not yet treat literature-backed multi-phase fault patterns as a first-class test input. Reproducing Jepsen- or TigerBeetle-style helical failures still requires hand-building schedules, which means the most interesting overlapping failure shapes are underused and hard to compare across runs.

## What Changes

- Add named helical scenario families that generate rotating, overlapping fault schedules across nodes and phases.
- Support storage-oriented helical scenarios that combine `DiskFsyncLie`, `DiskFsyncFlush`, `DiskSlow`, `DiskPartialRead`, `ProcessKill`, `ProcessRestart`, and partitions in one reusable campaign input.
- Integrate helical scenarios into the campaign runner and reports so seeds record the scenario family, parameters, and phase plan used.
- Make replay/minimize preserve the chosen helical scenario metadata while still reducing the concrete fault schedule for reproduction.

## Capabilities

### New Capabilities
- `helical-fault-campaigns`: Named multi-phase fault generators that rotate failures around the cluster instead of relying on isolated one-shot faults.
- `helical-storage-scenarios`: Reusable storage-focused helical patterns that stress volatile writes, restart boundaries, and degraded I/O alongside network faults.

### Modified Capabilities
- `campaign-runner`: Accept named helical scenarios, record scenario metadata in campaign output, and preserve it across resume/replay flows.

## Impact

- **Crates**: `chaoscontrol-explore` (scenario generator, CLI, reports, checkpoint metadata), `chaoscontrol-fault` (schedule builder helpers), and `chaoscontrol-replay` / minimization paths for metadata preservation.
- **Reports/Artifacts**: Campaign, bug, and replay artifacts gain scenario-family metadata and per-phase summaries.
- **Testing**: New generator tests for scenario determinism and campaign tests proving scenario metadata survives checkpoint, report, replay, and minimization flows.
- **No breaking API changes.** Existing direct fault schedules continue to work; helical scenarios are an additive higher-level input.
