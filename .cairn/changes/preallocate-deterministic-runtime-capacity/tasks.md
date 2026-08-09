## Phase 1: Baseline and capacity plan

- [x] [serial] `[runtime-capacity-baseline]` Record baseline scheduler, virtio-buffer, network-queue, and deterministic replay results before core changes. r[chaoscontrol.runtime_capacity.verification]
- [x] [depends:runtime-capacity-baseline] `[runtime-capacity-plan]` Define explicit capacity inputs, hard caps, checked arithmetic, slot states, and startup outcomes. r[chaoscontrol.runtime_capacity.plan]
- [x] [parallel] Add positive plans and negative zero, one-past-cap, contradiction, and arithmetic-overflow fixtures. r[chaoscontrol.runtime_capacity.plan] r[chaoscontrol.runtime_capacity.verification]

## Phase 2: Journal and pools

- [x] [depends:runtime-capacity-plan] `[runtime-capacity-journal]` Preallocate admitted schedule-record capacity and remove allocator calls from reservation. r[chaoscontrol.runtime_capacity.journal]
- [x] [depends:runtime-capacity-plan] `[runtime-capacity-virtio]` Add move-only virtio scratch-buffer leases with startup allocation, zeroing, exact return, and teardown checks. r[chaoscontrol.runtime_capacity.virtio_pool]
- [x] [depends:runtime-capacity-plan] `[runtime-capacity-network]` Add preallocated network packet slots and queue metadata with atomic bound checks before commit. r[chaoscontrol.runtime_capacity.network_pool]
- [x] [parallel] Preserve exact single-step scheduling, poison transitions, packet order, and current claim boundaries. r[chaoscontrol.runtime_capacity.boundary]

## Phase 3: Observation and verification

- [x] [depends:runtime-capacity-journal] `[runtime-capacity-observation]` Add bounded plan, allocation, high-water, exhaustion, release, and leak observations. r[chaoscontrol.runtime_capacity.observation]
- [x] [parallel] Add negative allocation-failure, exhaustion, stale-lease, duplicate-return, leaked-slot, and post-commit-fault tests. r[chaoscontrol.runtime_capacity.verification]
- [x] [parallel] Add a deterministic probe that rejects allocator calls in selected steady-state journal, scratch, and packet operations. r[chaoscontrol.runtime_capacity.verification]
- [ ] [serial] Run focused core and VMM tests, deterministic replay checks, formatting, Clippy, Cairn checks, and relevant Nix checks. r[chaoscontrol.runtime_capacity.verification]
