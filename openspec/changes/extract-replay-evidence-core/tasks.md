## Tasks

- [ ] Define the shared replay/evidence DTO boundary and compatibility adapter map. r[replay-verdicts.shared-core.dto-boundary]
- [ ] Add positive fixtures for current explorer verdicts, evidence readiness accepted proof records, snapshot-backed reproduced verdicts, schedule-only gaps, and no-bug classifications. r[replay-verdicts.shared-core.positive-fixtures]
- [ ] Add negative fixtures for malformed hashes, missing snapshot refs, invalid digests, path escapes, unsupported replay classes, stale artifact hashes, and overclaim wording. r[replay-verdicts.shared-core.negative-fixtures]
- [ ] Implement the pure shared core crate and keep filesystem, VM, clock, process, and Nickel orchestration in shell crates. r[replay-verdicts.shared-core.pure-core]
- [ ] Migrate `chaoscontrol-explore` and `chaoscontrol-evidence` call sites through compatibility adapters without changing public JSON fields. r[replay-verdicts.shared-core.compatibility]
- [ ] Wire focused Rust tests and evidence contract checks into the existing readiness validation surface. r[rust-owned-evidence-readiness.shared-core.validation]
