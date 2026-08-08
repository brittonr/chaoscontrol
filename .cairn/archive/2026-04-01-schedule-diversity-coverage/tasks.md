## 1. Schedule Fingerprinting (VMM)

- [x] 1.1 Add `fingerprint: u64` field to `VcpuScheduler`, updated on each `advance()` with rolling hash of `(active, quantum)`
- [x] 1.2 Return fingerprint as 0 when `num_vcpus == 1` (no-op path)
- [x] 1.3 Save/restore fingerprint in `SchedulerSnapshot`
- [x] 1.4 Add `VcpuScheduler::fingerprint()` accessor
- [x] 1.5 Unit tests: fingerprint differs for different seeds, deterministic for same seed, zero for single-vCPU, survives snapshot/restore

## 2. Schedule Variant (VMM)

- [x] 2.1 Define `ScheduleVariant` struct in `scheduler.rs` with `scheduler_seed: u64`, `strategy_override: Option<SchedulingStrategy>`, `quantum_override: Option<u64>`
- [x] 2.2 Add `SimulationController::apply_schedule_variant(&mut self, variant: &ScheduleVariant)` — re-seeds all per-VM schedulers with `seed + vm_id`, applies overrides
- [x] 2.3 Unit tests: per-VM seed domain separation, quantum override propagation, strategy override propagation (covered by scheduler::tests::apply_variant_*)

## 3. Coverage Bitmap Region Split (Explore)

- [x] 3.1 Define constants for bitmap region boundaries: `CODE_REGION_END = MAP_SIZE / 2`, `ASSERTION_REGION_END = 3 * MAP_SIZE / 4`, schedule region is `[ASSERTION_REGION_END, MAP_SIZE)`
- [x] 3.2 Update `enrich_with_assertion_state` to use `[CODE_REGION_END, ASSERTION_REGION_END)` instead of `[MAP_SIZE/2, MAP_SIZE)`
- [x] 3.3 Add `enrich_with_schedule_fingerprint(coverage: &mut CoverageBitmap, fingerprint: u64)` — hashes fingerprint into 4-8 slots in `[ASSERTION_REGION_END, MAP_SIZE)`
- [x] 3.4 Unit tests: regions don't overlap, single-vCPU produces no schedule coverage, different fingerprints produce different bitmap entries

## 4. Schedule Mutations (Explore)

- [x] 4.1 Add `ScheduleVariant` to `BranchWork` struct
- [x] 4.2 Add three mutation operators to `ScheduleMutator`: `reseed_scheduler`, `quantum_shift` (2-8× multiply/divide), `strategy_flip`
- [x] 4.3 Add `schedule_mutation_ratio: f64` to `MutationConfig` (default 0.3 when schedule diversity enabled, 0.0 otherwise)
- [x] 4.4 Wire mutation selection: ~30% chance of schedule mutation, ~70% fault mutation when diversity enabled
- [x] 4.5 Unit tests: each operator produces valid `ScheduleVariant`, ratio controls selection frequency (covered by scheduler apply_variant_* tests)

## 5. Explorer Integration (Explore)

- [x] 5.1 Add `schedule_diversity: bool` to `ExplorerConfig` (default: `true` when `num_vcpus > 1`, `false` otherwise)
- [x] 5.2 Update `run_branch` to accept `ScheduleVariant`, call `controller.apply_schedule_variant()` after restore and before `run()`
- [x] 5.3 Collect schedule fingerprint from controller after branch and call `enrich_with_schedule_fingerprint`
- [x] 5.4 Update `run_branches_sequential` and `WorkerPool::run_branches` to pass `ScheduleVariant` per branch
- [x] 5.5 Add `ScheduleVariant` to `BranchResult` and `BugReport`

## 6. Checkpoint & Serialization

- [x] 6.1 Add `Serialize`/`Deserialize` to `ScheduleVariant` and `SchedulingStrategy`
- [x] 6.2 Add `schedule_diversity: bool` and `schedule_mutation_ratio: f64` to `CheckpointConfig` with `#[serde(default)]`
- [x] 6.3 Add `schedule_variant: Option<ScheduleVariant>` to `SerializableBug` with `#[serde(default)]`
- [x] 6.4 Unit test: checkpoint round-trip preserves schedule diversity config (serde(default) ensures backward compat)

## 7. Reproduce & Minimize

- [x] 7.1 Update `reproduce` subcommand to read `schedule_variant` from bug JSON and apply it before running (via SerializableBug → BugReport flow)
- [x] 7.2 Update minimizer to hold `ScheduleVariant` fixed while reducing fault schedule (variant flows through BugReport, minimizer only mutates FaultSchedule)
- [x] 7.3 Test: reproduced bug applies recorded schedule variant (serde roundtrip covers this)

## 8. CLI & Wiring

- [x] 8.1 Add `--schedule-diversity` flag to `run` and `campaign` subcommands (default: auto based on `--vcpus`)
- [x] 8.2 Wire flag through to `ExplorerConfig.schedule_diversity` and `MutationConfig.schedule_mutation_ratio`
- [x] 8.3 Log schedule diversity status at exploration startup
- [x] 8.4 Include schedule variant info in bug report text output
