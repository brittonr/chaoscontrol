## Phase 1: Dependency and baseline

- [x] [serial] Pin Cargo and Nix to VM Cohort `ab123e3673b6dd616b3df5d044026b5e85755149` and verify source agreement. r[chaoscontrol.vm_cohort.source]
- [x] [serial] Retain the reviewed snapshot, block-overlay, divergence, exact-KVM, and negative baseline. r[chaoscontrol.vm_cohort.parity]
- [x] [parallel] Add dependency gates that reject moving refs, sibling paths, and consumer-domain leakage into VM Cohort. r[chaoscontrol.vm_cohort.authority]

## Phase 2: Adapter and parity

- [x] [serial] Map exact snapshot, initialized memory, initialized disk, topology, runtime, and adapter facts into VM Cohort checkpoint and cohort inputs. r[chaoscontrol.vm_cohort.adapter]
- [x] [serial] Apply ChaosControl-owned exact vCPU and in-kernel device state through VM Cohort clone descriptors before activation. r[chaoscontrol.vm_cohort.restore]
- [x] [parallel] Compare legacy and shared reads, writes, snapshots, restores, divergence, dirty-page counts, and bounded error classes. r[chaoscontrol.vm_cohort.parity]
- [x] [parallel] Add profile drift, base mismatch, shared mutable state, partial creation, unknown cleanup, and policy-leak negative cases. r[chaoscontrol.vm_cohort.verification]

## Phase 3: Selection and closeout

- [x] [serial] Select the shared cohort mechanism and label the existing path as diagnostic rollback only. r[chaoscontrol.vm_cohort.selection]
- [x] [serial] Document dependency, mapping, parity, rollback, authority, and evidence non-claims. r[chaoscontrol.vm_cohort.authority]
- [x] [serial] Run focused and broad Cargo tests, KVM smoke, Clippy, Octet, Cairn gates, lifecycle checks, and relevant Nix checks. r[chaoscontrol.vm_cohort.verification]
