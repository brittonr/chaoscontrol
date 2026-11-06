## Tasks

- [x] [serial] Record the 2026-09-25 round-one and round-two divergence between identical Raft runs. r[chaoscontrol.explorer_repeatability.gate]
- [x] [depends:repeatability-baseline] Add branch digests to branch results and round history with old-checkpoint defaults. r[chaoscontrol.explorer_repeatability.branch_digest]
  - Evidence: `BranchResult::canonical_digest` binds coverage bytes, canonical oracle report, per-VM exit counts, total ticks, and the halted flag under a domain-separated BLAKE3 identity, excluding wall-clock timings. `RoundReport.branch_digests` and `RoundHistory.branch_digests` carry admitted branch digests in execution order for sequential, worker-pool, and input-tree rounds; the history field is `#[serde(default)]`, so old checkpoints read as empty. `branch_digest_binds_result_facts_and_ignores_timings` passes.
- [x] [depends:repeatability-digest] Add the pure history comparison core with matching, mismatching, and operational fixtures. r[chaoscontrol.explorer_repeatability.gate] r[chaoscontrol.explorer_repeatability.operational_exclusion]
  - Evidence: `chaoscontrol_explore::repeatability::compare` returns typed `Match`, `RoundCountMismatch`, `Mismatch { round, branch, reason }`, `Operational { round, reason }`, or `Unavailable { round }`. Watchdog firings, discarded branches, and failed branches are excluded before digest comparison; empty old-checkpoint digests are `Unavailable`, never a match. Five tests cover matching, first divergent branch, branch/round count differences, each operational exclusion, and missing digests.
- [x] [depends:repeatability-core] Add the fresh-process gate runner and receipt for the Raft control and one planted variant. r[chaoscontrol.explorer_repeatability.gate]
  - Evidence: [gate runner](evidence/2026-11-05-gate-runner.md). `repeatability_gate::run_gate` spawns two fresh explorer processes for one recorded configuration, compares checkpoint round histories through the pure comparison core, and writes a non-overwriting receipt with checkpoint digests and operational/unavailable counts. The CLI gate run for the Raft control (`seed 42`, 3 rounds × 4 branches × 1000 ticks) returned `Mismatch { round: 1, branch: Some(0), reason: BranchDigest }` with zero operational rounds, reproducing the defect and naming the first divergent branch. Four unit tests cover facts, verdict exit mapping, output naming, and receipt round-trip.
- [ ] [depends:repeatability-gate] After `repair-incremental-snapshot-lineage` lands, locate the first divergent branch and add a focused regression for each diagnosed cause. r[chaoscontrol.explorer_repeatability.cause]
- [ ] [depends:repeatability-cause] Fix each cause, rerun the gate until every maintained case matches, and run explorer tests, KVM gate runs, and Cairn validation. r[chaoscontrol.explorer_repeatability.cause] r[chaoscontrol.explorer_repeatability.gate]

## Remaining blockers

- The fresh-process gate runner and receipt require the Raft kernel/initrd and a
  working KVM campaign cohort; no built guest artifacts exist locally.
- Cause diagnosis and the maintained-case rerun are gated on
  `repair-incremental-snapshot-lineage`, whose campaign tasks remain open.
