# Deterministic runtime capacity

ChaosControl admits selected runtime capacity before VM activation. The pure simulation core checks the plan and derives its BLAKE3 identity.

The current plan covers these resources:

- Schedule-journal record storage.
- One scratch-buffer pool for each virtio block, network, and entropy device.
- Retained virtio network TX packet slots.
- Network TX queue metadata.

The VM stores the admitted plan. `DeterministicVm::runtime_capacity_observations()` reports the plan, startup result, usage, high-water values, exhaustion counts, releases, and leaks.

## Schedule journal

`ScheduleJournal::new()` allocates the admitted record capacity. A transition reservation only checks the logical limit and records the reservation identity.

A reservation and commit do not increase the vector capacity. A trace drain clones the published trace, then clears the retained journal storage.

The clone can allocate during evidence publication. The no-allocation claim applies only to schedule reservation and commit after initialization.

## Virtio scratch buffers

Each selected virtio device allocates one scratch slot before activation. A move-only lease removes the buffer from its pool for one request.

The pool clears the requested bytes before use. The pool clears the complete slot again before release.

The pool rejects these conditions:

- The request is larger than the selected slot.
- No matching slot is free.
- The lease has a stale generation.
- The slot identity or capacity is invalid.
- Startup allocation fails.

An operation error still returns and clears the lease.

## Network TX packet slots

The network backend allocates packet storage and queue metadata before activation. TX processing copies guest bytes into one free slot before queue commit.

Packet-count, byte-count, frame-size, and free-slot checks occur before counter changes. The queue keeps FIFO order by storing slot indices.

A queue drain copies packets into the caller output. This output publication can allocate. It also clears and releases each packet slot.

## Evidence validation

`validate_runtime_capacity_observations()` binds observations to the exact plan identity. It checks allocated capacity, current use, high-water values, retained bytes, and leak counts.

The validator rejects these claims:

- Deterministic latency.
- Zero allocation for the complete process.
- Zero-copy I/O.
- Guaranteed host memory.

The evidence only describes the selected capacity plan and observed pool transitions.

## Failure scope

A startup allocation error stops VM construction before guest progress. Capacity exhaustion is a typed runtime result for the selected pool.

Post-commit device errors keep the existing poison rules. Capacity handling does not weaken schedule, queue, snapshot, replay, or poison authority.
