## Tasks

- [x] [serial] Record the 2026-09-25 round-one and round-two divergence between identical Raft runs. r[chaoscontrol.explorer_repeatability.gate]
- [ ] [depends:repeatability-baseline] Add branch digests to branch results and round history with old-checkpoint defaults. r[chaoscontrol.explorer_repeatability.branch_digest]
- [ ] [depends:repeatability-digest] Add the pure history comparison core with matching, mismatching, and operational fixtures. r[chaoscontrol.explorer_repeatability.gate] r[chaoscontrol.explorer_repeatability.operational_exclusion]
- [ ] [depends:repeatability-core] Add the fresh-process gate runner and receipt for the Raft control and one planted variant. r[chaoscontrol.explorer_repeatability.gate]
- [ ] [depends:repeatability-gate] After `repair-incremental-snapshot-lineage` lands, locate the first divergent branch and add a focused regression for each diagnosed cause. r[chaoscontrol.explorer_repeatability.cause]
- [ ] [depends:repeatability-cause] Fix each cause, rerun the gate until every maintained case matches, and run explorer tests, KVM gate runs, and Cairn validation. r[chaoscontrol.explorer_repeatability.cause] r[chaoscontrol.explorer_repeatability.gate]
