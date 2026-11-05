## Tasks

- [x] [serial] Review the per-exit clock, PMU prerequisite, and guest determinism time boundary; record the gated design. r[chaoscontrol.tsc_progress.mode_gate]
- [x] [serial] Model per-exit versus counter time on synthetic busy-loop and exit-frequency traces with controlled timeouts. r[chaoscontrol.tsc_progress.mode_gate]
- [x] [serial] Add the profile-selected mode to admission with rejection while PMU support is unadmitted; cover default and gated cases. r[chaoscontrol.tsc_progress.mode_gate]
- [ ] [serial] Bind mode, base, scale, and last retired count into snapshot state; reject cross-mode restore and prove same-mode continuation equality. r[chaoscontrol.tsc_progress.snapshot_consistency]
- [x] [serial] Define HLT deadline and PIT `tsc_khz` behavior for the mode and cover both with unit-level boundary tests. r[chaoscontrol.tsc_progress.snapshot_consistency]
- [x] [serial] Measure per-exit skid stability across snapshot repeats on an admitted host; reject the host profile if skid is unstable. r[chaoscontrol.tsc_progress.mode_gate]
- [ ] [serial] Build the busy-loop TSC-timeout fixture, record per-exit versus progress outcomes, and admit the mode only on bit-exact repeat expiry. r[chaoscontrol.tsc_progress.mode_gate]
- [ ] [serial] Update clock documentation and the product-scope intent for the evaluated mode without promoting PMU support. r[chaoscontrol.tsc_progress.mode_gate]

## Evidence

Detached model `tools/counter-time-model` (four tests) shows the exit clock cannot
fire a TSC timeout in a CPU-bound loop with no exits while the counter clock fires
at the declared instruction budget, and that the exit clock can fire on exit
frequency alone. This is model-level evidence only; no KVM run, PMU measurement,
skid, or runtime mode exists.

Host profile measured on 2026-11-05: AMD Ryzen 9 9950X3D, family 26 model 68
stepping 0, Linux 6.18.50, `/dev/kvm` present, `perf_event_paranoid=-1`.
`pmu_kvm_test` booted the admitted vmlinux/initrd to 5000 exits, snapshotted once, and
replayed 5 runs x 500 exits with `exclude_host=1`: 1990/2000 comparisons were
bit-exact, p50/p90/p99 delta 0, max |delta| 6, with +6/-6 and +1/-1 outliers. The
tool's verdict is near-deterministic, not bit-exact. Per the design gate this host
profile is rejected for progress mode, so `INSTRUCTION_PROGRESS_TSC_ADMITTED`
remains false and the busy-loop admission task stays unchecked. This measures one
host and one workload; it does not generalize to other hosts or guests.

Design-only foundation. The per-exit clock is `DEFAULT_TSC_ADVANCE`/`VirtualTsc::tick`
in `chaoscontrol-vmm`; the candidate progress source is the guest-only
retired-instruction PMU the scheduler consumes. `harden-pmu-transition-evidence` is
an active spec-only package with every implementation task unchecked, and the
release matrix retains `claims_pmu_support = false`, so runtime work stays blocked.
`guest-os-determinism-boundary` owns the admitted guest time surface; this change
does not redefine it. Open risks recorded: per-exit skid stability, HLT deadline
semantics under a halted vCPU, and PIT/TSC mapping consistency. No measurement,
KVM run, or determinism claim is established here.
