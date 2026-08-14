## Tasks

- [x] [serial] Define the required KVM matrix, worker facts, terminal classes, and bounded release claim. r[chaoscontrol.kvm_release_rail.matrix] r[chaoscontrol.kvm_release_rail.boundary]
- [ ] [depends:kvm-rail-foundation] Add the typed Nickel matrix and deterministic runtime projection. r[chaoscontrol.kvm_release_rail.matrix]
- [ ] [depends:kvm-release-matrix] Implement pure capability, freshness, row, artifact, and terminal verdict classification. r[chaoscontrol.kvm_release_rail.functional_core]
- [ ] [depends:kvm-release-core] Add the thin KVM worker runner with explicit source, host, command, limit, and artifact observations. r[chaoscontrol.kvm_release_rail.worker]
- [ ] [depends:kvm-release-runner] Wire exact SMP, snapshot, virtio safety, drift, and fresh workload replay rows. r[chaoscontrol.kvm_release_rail.required_rows]
- [ ] [depends:kvm-release-rows] Emit one bounded receipt and summary for the complete matrix. r[chaoscontrol.kvm_release_rail.receipt]
- [ ] [parallel] Add positive complete-matrix fixtures and negative missing, stale, skipped, unsupported, timeout, tampered, dirty, and overclaim fixtures. r[chaoscontrol.kvm_release_rail.validation]
- [ ] [depends:kvm-release-validation] Add separate portable and KVM CI lanes with bounded artifact retention. r[chaoscontrol.kvm_release_rail.ci]
- [ ] [depends:kvm-release-ci] Run focused core, KVM, CI dry-run, Cairn, and relevant Nix validation. r[chaoscontrol.kvm_release_rail.validation]
