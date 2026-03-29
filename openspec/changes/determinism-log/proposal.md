## Why

When determinism breaks, there's no good way to find *where*. Today you
either re-run under eBPF tracing (requires sudo, external tooling, can't
capture VMM-internal state like scheduler decisions) or sprinkle
`info!()` calls and rebuild. Neither scales to production exploration
runs that execute millions of VM exits across dozens of seeds.

A structured, binary event log recorded inside the VMM itself — every
exit, every scheduler switch, every SDK hypercall — lets you run two
identical seeds side-by-side and diff them to the exact exit where they
diverge. Zero overhead when disabled, ~5% overhead when enabled.

## What Changes

- New `DeterminismLog` in `chaoscontrol-vmm` that captures a fixed-size
  record per VM exit: exit type, port/address, data, virtual TSC, exit
  count, active vCPU, RIP.
- Additional record types for scheduler switches, SDK hypercalls,
  interrupt injections, fault applications, and snapshot/restore events.
- Binary format written via `BufWriter<File>` — one flat file per VM,
  ~64 bytes per record, designed for sequential scan and comparison.
- `DlogReader` for sequential iteration, text dump, and two-file
  divergence comparison.
- CLI integration: `chaoscontrol-explore run --dlog <dir>` enables
  logging. `chaoscontrol-replay dlog-diff <a> <b>` finds first
  divergence between two logs.
- `VmConfig.dlog_path: Option<PathBuf>` controls enablement per VM.
  When `None`, every callsite is a branch-on-Option that costs nothing
  measurable.

## Capabilities

### New Capabilities
- `determinism-log`: Binary per-exit event log inside the VMM for
  diagnosing determinism regressions. Covers record format, writer,
  reader, diff tool, and CLI integration.

### Modified Capabilities
(none — this is additive instrumentation; no existing behavior changes)

## Impact

- **crates/chaoscontrol-vmm**: New `dlog.rs` module. `DeterministicVm`
  gains an `Option<DlogWriter>` field. `step()`, `run_bounded()`,
  `handle_sdk_hypercall()`, and scheduler paths emit records when
  logging is active.
- **crates/chaoscontrol-vmm/src/vm.rs**: ~15 callsites in the main exit
  loop, each guarded by `if let Some(dlog) = &mut self.dlog`.
- **crates/chaoscontrol-explore**: Wire `--dlog` flag through
  `ExplorerConfig` → per-VM `VmConfig`.
- **crates/chaoscontrol-replay**: New `dlog-diff` subcommand.
- **Dependencies**: None new — uses `std::io::BufWriter` and existing
  serde for the text dump path.
