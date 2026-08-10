## Context

Strict assertion identity correctly rejects every retained workload proof. Existing snapshots remain useful for diagnosis, but they cannot support current promotion.

## Decisions

### 1. One proof cohort owns all admission facts

A typed Nickel profile binds the source revision, guest and host artifacts, KVM capability profile, assertion catalog, run configuration, snapshot policy, replay policy, named bounds, and evidence scope. Runtime assertion, checkpoint, bug, and verdict records remain Rust-owned.

### 2. Raft is the first promotion target

The first accepted result must be one complete Raft run with strict identity and snapshot-backed reproduction. Later workloads must use the same admission rules.

### 3. Fresh evidence cannot inherit legacy authority

Schema-v1 IDs and historical receipts remain diagnostic-only. New execution must create the strict catalog, bug carrier, replay verdict, and receipt from current code and artifacts.

### 4. Promotion is a pure decision

The replay evidence core receives loaded manifests and observations. It returns typed eligibility, blockers, and diagnostics. Shells own KVM, file access, artifact materialization, and publication.

### 5. Onboarding ends at a classified result

The Rust workload flow must create a scaffold, build the guest, run a bounded campaign, replay a selected failure, and emit a promotion decision. A no-bug run remains a valid diagnostic result, not accepted replay proof.

### 6. Claims stay bounded

A promoted proof establishes only the recorded workload, artifact cohort, schedule, snapshot, assertion, and replay outcome. It does not prove workload correctness or general hypervisor determinism.

## Validation

Positive cases cover strict catalogs, fresh artifacts, reproduced snapshot verdicts, one-command Rust onboarding, and all four workload rows. Negative cases cover legacy IDs, stale revisions, catalog conflicts, wrong snapshots, hash drift, incomplete receipts, no-KVM hosts, and claim promotion.
