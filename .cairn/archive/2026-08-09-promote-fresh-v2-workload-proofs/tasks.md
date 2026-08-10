## Tasks

- [x] [serial] Define the fresh proof cohort, workload order, strict identity boundary, and bounded claim model. r[chaoscontrol.fresh_workload_proofs.profile] r[chaoscontrol.fresh_workload_proofs.boundary]
- [x] [depends:fresh-proof-foundation] Add the typed Nickel proof cohort and deterministic runtime projection. r[chaoscontrol.fresh_workload_proofs.profile]
- [x] [depends:fresh-proof-profile] Implement pure fresh-proof admission and blocker classification in the replay evidence core. r[chaoscontrol.fresh_workload_proofs.admission] r[chaoscontrol.fresh_workload_proofs.functional_core]
- [x] [depends:fresh-proof-admission] Produce one fresh Raft schema-v2 KVM run, snapshot replay, verdict, receipt, and accepted manifest entry. r[chaoscontrol.fresh_workload_proofs.raft_first]
- [x] [depends:fresh-raft-proof] Repeat the admitted proof path for Redb, network, and the downstream-shaped Rust workload. r[chaoscontrol.fresh_workload_proofs.coverage]
- [x] [depends:fresh-proof-profile] Add one command that takes a Rust scaffold through build, bounded KVM run, replay, and promotion classification. r[chaoscontrol.fresh_workload_proofs.onboarding]
- [x] [parallel] Add positive strict/fresh/reproduced fixtures and negative legacy, stale, conflicting, tampered, missing, no-KVM, and overclaim fixtures. r[chaoscontrol.fresh_workload_proofs.validation]
- [x] [depends:fresh-workload-coverage] Regenerate proof coverage, assertion readiness, replay readiness, and onboarding documents from admitted artifacts. r[chaoscontrol.fresh_workload_proofs.coverage]
- [x] [depends:fresh-proof-validation] Run focused Rust, Nickel, KVM, replay, evidence, Cairn, and relevant Nix validation. r[chaoscontrol.fresh_workload_proofs.validation]
