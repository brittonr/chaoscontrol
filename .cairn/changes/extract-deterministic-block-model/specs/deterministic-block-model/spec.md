# Deterministic Block Model Specification

## Purpose

Defines a product-neutral copy-on-write block model for deterministic simulation and storage tests.

## Requirements

### Requirement: The block model is an independent shared package

r[shared.deterministic_block.repository] The shared `deterministic-sim` repository MUST publish `deterministic-block` as an independent `AGPL-3.0-or-later` package. The package MUST NOT require KVM, virtio, guest-memory, scheduler, or evidence crates.

#### Scenario: A storage test adopts the package

- GIVEN the shared package passes its behavior and package checks
- WHEN a consumer adds only `deterministic-block`
- THEN it MUST be able to construct and operate the model without KVM or a runtime scheduler
- AND it MUST pin an immutable reviewed shared revision.

### Requirement: Geometry and resource limits are explicit

r[shared.deterministic_block.geometry] Construction MUST require named logical-block, copy-on-write-page, capacity, transfer, dirty-page, and allocation limits. It MUST check divisibility, representability, and capacity invariants before state creation.

#### Scenario: Page geometry is incompatible

- GIVEN a copy-on-write page size is incompatible with logical block geometry or capacity
- WHEN construction checks the geometry
- THEN it MUST return a typed geometry failure
- AND it MUST NOT create partial device state.

### Requirement: Operations use pure plans before mutation

r[shared.deterministic_block.planning] Read, write, flush, reset, and fault operations MUST first produce a pure checked plan over supplied state facts. Invalid ranges, arithmetic, geometry, faults, or resource use MUST fail before buffer or overlay mutation.

#### Scenario: A write range overflows

- GIVEN an offset and length whose checked end cannot be represented or exceeds capacity
- WHEN write planning runs
- THEN it MUST return a typed range failure
- AND no dirty page, durable page, or caller buffer MAY change.

### Requirement: Storage layers have explicit precedence

r[shared.deterministic_block.layers] The model MUST use an immutable shared base, durable overlay, and volatile overlay with documented read precedence. Flush and crash transitions MUST define exactly which layer changes and MUST apply one accepted plan once.

#### Scenario: Volatile and durable pages overlap

- GIVEN one page has bytes in both overlays
- WHEN a read plan resolves that range
- THEN volatile bytes MUST take precedence before flush
- AND the documented durable state MUST take precedence after an accepted flush removes the volatile entry.

#### Scenario: A crash transition runs

- GIVEN volatile writes exist above a valid durable and base state
- WHEN the declared crash transition applies
- THEN volatile state MUST be discarded according to policy
- AND durable and base state MUST remain unchanged.

### Requirement: Fault plans are explicit inputs

r[shared.deterministic_block.faults] Read failure, write failure, torn extent, and corruption position MUST enter as explicit typed plans. The block package MUST NOT read entropy, select schedules, or infer fault authority.

#### Scenario: A torn-write plan exceeds the request

- GIVEN a write request and a torn extent outside that request
- WHEN fault planning checks the operation
- THEN it MUST return a typed fault-plan failure
- AND no prefix, suffix, or overlay bytes MAY change.

#### Scenario: A valid torn write applies

- GIVEN an accepted write and a valid explicit torn extent
- WHEN the in-memory shell applies the plan
- THEN only the declared bytes MUST enter the volatile overlay
- AND the outcome MUST report the exact applied extent.

### Requirement: Snapshots preserve complete model state

r[shared.deterministic_block.snapshot] A versioned snapshot MUST preserve geometry, limits required for compatibility, immutable-base identity facts, durable overlay, volatile overlay, and deterministic counters. Restore preflight MUST reject incomplete or incompatible state before reconstruction.

#### Scenario: Snapshot geometry differs from the destination

- GIVEN a snapshot and destination use incompatible block or page geometry
- WHEN restore preflight runs
- THEN it MUST return a typed compatibility failure
- AND destination state MUST remain unchanged.

### Requirement: External I/O remains outside the model

r[shared.deterministic_block.shell_boundary] The block package MUST consume admitted base bytes or a bounded byte source. It MUST NOT open paths, traverse directories, map ambient files, decompress artifacts, or decide artifact trust.

#### Scenario: A disk image path is supplied

- GIVEN ChaosControl receives an operator disk-image path
- WHEN it creates the shared block model
- THEN a ChaosControl or bounded-input shell MUST admit and read the bytes first
- AND the block model MUST receive only admitted bytes and geometry facts.

### Requirement: ChaosControl owns VMM behavior

r[shared.deterministic_block.chaoscontrol_boundary] Virtio queues, guest memory access, MMIO state, interrupts, fault scheduling, artifact storage, replay evidence, and durability claims MUST remain in ChaosControl adapters.

#### Scenario: A virtio write reaches the backend

- GIVEN ChaosControl has validated a complete virtio request
- WHEN its adapter calls the shared block model
- THEN the shared model MUST return a storage plan and outcome only
- AND ChaosControl MUST remain responsible for guest completion and interrupt behavior.

### Requirement: Migration preserves bytes and transitions

r[shared.deterministic_block.migration] Before local block logic is removed, old and shared implementations MUST produce equal read bytes, overlay transitions, fault outcomes, snapshots, and post-restore continuation for maintained fixtures.

#### Scenario: Overlay state differs after flush

- GIVEN a maintained write and flush fixture
- WHEN old and shared state is compared
- THEN any durable or volatile overlay difference MUST block migration
- AND local code MUST remain until an explicit behavior change is accepted.

### Requirement: Checks cover positive and negative storage behavior

r[shared.deterministic_block.validation] Shared and ChaosControl suites MUST cover valid construction, reads, writes, flushes, crashes, faults, snapshots, and restores plus malformed geometry, overflow, range, capacity, allocation, fault-plan, and compatibility failures.

#### Scenario: Full block checks run

- GIVEN shared unit fixtures and ChaosControl block and virtio fixtures
- WHEN all focused checks run
- THEN accepted operations MUST produce deterministic bytes and state
- AND rejected operations MUST fail without panic, unbounded allocation, partial mutation, or false successful completion.
