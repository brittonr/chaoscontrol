## Tasks

- [x] [serial] Confirm the unwired variant path in explorer branch dispatch and sequential execution. r[chaoscontrol.schedule_diversity.generation] r[chaoscontrol.schedule_diversity.application]
- [x] [depends:schedule-diversity-baseline] Route scheduled branch generation through the schedule-aware mutator when diversity is enabled. r[chaoscontrol.schedule_diversity.generation]
- [x] [depends:schedule-diversity-generation] Pass the variant through `BranchWork` in both the parallel pool and the sequential path and apply it before each branch run. r[chaoscontrol.schedule_diversity.application]
- [x] [depends:schedule-diversity-application] Bind variant policy bytes into the schedule fingerprint, bug reports, and replay verdicts. r[chaoscontrol.schedule_diversity.evidence_identity]
- [x] [depends:schedule-diversity-identity] Add a fixture race workload with a known triggering interleaving and a gate that requires detection. r[chaoscontrol.schedule_diversity.validated_effectiveness]
- [x] [parallel] Add negative fixtures for disabled diversity, single-vCPU, unsupported strategy, and identity drift. r[chaoscontrol.schedule_diversity.validation]
- [x] [depends:schedule-diversity-validation] Run focused core, VM, replay, Cairn, and relevant Nix validation. r[chaoscontrol.schedule_diversity.validation]
