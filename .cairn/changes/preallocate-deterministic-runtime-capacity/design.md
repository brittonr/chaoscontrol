## Context

The scheduler journal has an explicit record limit, but its vector starts without capacity. Each reservation can call the allocator before guest progress.

Virtio scratch buffers allocate one vector for each request. Network retention accepts owned packet vectors and grows queue storage during active execution.

The change must preserve the deterministic core and imperative shell boundary. It must also preserve existing poisoning and evidence rules.

## Decisions

### 1. A pure plan owns capacity arithmetic

**Choice:** Add a pure capacity plan over explicit hard caps and selected runtime limits.

The plan contains record slots, scratch-buffer classes, scratch leases, packet slots, packet bytes, and queue metadata slots. It uses checked arithmetic.

**Rationale:** Initialization must reject invalid or unrepresentable capacity before allocation or guest progress.

### 2. The shell allocates the complete selected capacity before activation

**Choice:** Allocate every planned pool before the VM or controller becomes active. A failed allocation returns a typed startup error.

**Rationale:** Runtime admission must not depend on later vector growth for selected hot paths.

### 3. Schedule reservation becomes allocation-free

**Choice:** `ScheduleJournal::new()` reserves its admitted record limit. `reserve()` checks logical capacity and records one reservation without allocator calls.

**Rationale:** Reservation occurs immediately before guest progress and must have stable failure behavior.

### 4. Scratch buffers use move-only leases

**Choice:** The shell owns fixed buffer slots by size class. A lease identifies one generation and slot and returns it exactly once.

Buffers are zeroed before exposure. Stale, duplicate, wrong-generation, and oversized returns fail closed.

**Rationale:** A lease makes ownership and cleanup obligations explicit without exposing unsafe memory operations.

### 5. Network retention separates packet storage from queue metadata

**Choice:** Preallocate packet slots and queue metadata. Copy admitted packet bytes into owned slots before queue commit.

A packet limit, byte limit, or free-slot limit fails before queue counters change. Post-commit fault injection keeps its existing poison behavior.

**Rationale:** Queue bounds alone do not prevent packet or metadata allocation during execution.

### 6. Observations do not become timing claims

**Choice:** Record capacity-plan identity, startup disposition, high-water usage, exhaustion counts, and leaked-slot diagnostics.

Do not report deterministic latency, zero-copy behavior, allocator absence outside the selected paths, or host memory guarantees.

## Functional Core and Imperative Shell

The pure core checks plans, transitions slot states, checks queue accounting, and classifies failures.

The shell allocates memory, zeroes buffers, copies packet bytes, owns leases, and releases resources during teardown.

## Risks and Trade-offs

- Full capacity increases startup memory and can reject configurations that previously failed later.
- Buffer pools add ownership states and cleanup work.
- Fixed packet slots can waste memory when traffic uses small packets.
- Pooling can retain sensitive bytes unless zeroing remains mandatory.

## Validation

Positive tests cover exact-limit initialization, repeated reserve and release, buffer reuse, packet FIFO order, and unchanged deterministic traces.

Negative tests cover arithmetic overflow, allocation failure, one-past-limit plans, exhaustion, stale leases, duplicate returns, leaked slots, and post-commit faults.

A test allocator or equivalent deterministic probe must detect allocation attempts in the selected steady-state operations.
