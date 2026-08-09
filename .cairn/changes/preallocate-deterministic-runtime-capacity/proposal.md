## Why

ChaosControl checks limits before deterministic guest progress, but selected runtime structures still allocate during execution.

`ScheduleJournal::reserve()` can grow its record vector before each transition. Virtio scratch buffers and retained network packets also allocate after runtime activation.

A late allocation failure can stop an admitted run at a host-dependent point. It can also add latency variation to deterministic control paths.

## What Changes

- Add an explicit runtime-capacity plan for schedule records, virtio scratch buffers, and retained network packets.
- Compute and check the plan before VM or controller activation.
- Allocate selected capacity during initialization and fail before guest progress when capacity is unavailable.
- Make schedule reservation allocation-free after successful initialization.
- Add lease-based scratch-buffer and packet-slot pools with explicit exhaustion outcomes.
- Record requested, admitted, allocated, used, exhausted, and released capacity as bounded runtime observations.

## Impact

- **Core**: pure capacity-plan checks, pool accounting, slot-state transitions, and outcome classification.
- **Shell**: fallible startup allocation, buffer zeroing, packet copying, lease return, and teardown.
- **Tests**: positive capacity use, negative startup failure, exhaustion, release, stale lease, and post-activation allocation detection.
- **Evidence**: capacity observations remain scoped runtime facts and do not prove deterministic latency or host memory availability.

## Non-Goals

- Do not claim that ChaosControl performs no dynamic allocation after startup.
- Do not change exact single-step guest scheduling or its evidence contract.
- Do not preallocate snapshots, exploration corpora, reports, or other non-hot-path artifacts.
- Do not add direct I/O, `io_uring`, native wire structs, or a new allocator.
- Do not treat configured capacity as proof that the host will preserve memory or latency.
