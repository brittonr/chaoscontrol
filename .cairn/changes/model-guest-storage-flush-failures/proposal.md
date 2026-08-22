# Proposal: Model guest storage flush failures

## Why

ChaosControl models write errors, torn writes, corruption, partial reads, full disks, slow disks, and lying synchronization. Its current virtio block request planner accepts read and write requests only.

Without a guest-visible flush operation, campaigns cannot bind an application `fsync` to one exact device flush request. The current Raft guest also keeps persistent state in memory and cannot test disk corruption across node restart.

## What Changes

- Add validated `VIRTIO_BLK_T_FLUSH` request planning and execution.
- Advertise the matching virtio feature only when the device supports it.
- Add deterministic flush success, immediate error, delayed error, and lying-flush outcomes.
- Preserve separate durable, volatile, and page-cache observations.
- Add process restart with cache retained, cache eviction, and host reboot profiles.
- Give each Raft guest node its own persistent disk image.
- Add protocol-aware storage recovery oracles for committed-entry preservation and unsafe rejoin.
- Add participant-scoped recovery progress for local sufficiency, peer-available repair, global absence, and incomplete observation.
- Emit bounded receipts with exact kernel, filesystem, device, workload, fault, and candidate identities.

## Impact

- **Core**: request-shape validation, fault schedules, cache-state transitions, and recovery oracles.
- **Shell**: virtio request execution, guest lifecycle, disk image persistence, and evidence capture.
- **Guests**: Redb single-node and persistent multi-node Raft workloads.

## Non-goals

- Do not claim equivalence with every storage device or kernel.
- Do not claim that direct I/O, checksums, or one filesystem fixes all failures.
- Do not make the VMM decide Raft commitment or repair authority.
- Do not treat a passing campaign as whole-system correctness or release readiness.
