## Tasks

- [x] Define the shared replay/evidence DTO boundary and compatibility adapter map. r[replay-verdicts.shared-core.dto-boundary]
- [x] Add positive fixtures for current explorer verdicts, evidence readiness accepted proof records, snapshot-backed reproduced verdicts, schedule-only gaps, and no-bug classifications. r[replay-verdicts.shared-core.positive-fixtures]
- [x] Add negative fixtures for malformed hashes, missing snapshot refs, invalid digests, path escapes, unsupported replay classes, stale artifact hashes, and overclaim wording. r[replay-verdicts.shared-core.negative-fixtures]
- [x] Implement the pure shared core crate and keep filesystem, VM, clock, process, and Nickel orchestration in shell crates. r[replay-verdicts.shared-core.pure-core]
- [x] Migrate `chaoscontrol-explore` and `chaoscontrol-evidence` call sites through compatibility adapters without changing public JSON fields. r[replay-verdicts.shared-core.compatibility]
- [x] Wire focused Rust tests and evidence contract checks into the existing readiness validation surface. r[rust-owned-evidence-readiness.shared-core.validation]
