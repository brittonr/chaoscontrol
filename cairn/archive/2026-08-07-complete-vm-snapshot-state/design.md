## Context

`VcpuSnapshot::capture` currently reads general, special, legacy FPU, debug, LAPIC, XCR, and MP state. It does not read KVM vCPU events, the required migratable MSR set, or XSAVE state. `VirtioDeviceSnapshot` contains only `device_id` and an optional block backing snapshot. `DeterministicVm::restore` zips those records with live devices and restores only block backing data, so a count or ordering mismatch can be truncated and all transport, queue, net, and virtio-rng mutations survive restore.

The runtime needs a checked state model before the shell performs KVM ioctls or mutates device backends.

## Decisions

### 1. Make completeness an explicit state inventory

**Choice:** Introduce a pure VM-state inventory derived from normalized VM configuration and captured host capability facts. The inventory lists every required component, per-vCPU entry, in-kernel device, VMM counter, virtio transport, queue, and backend instance. A snapshot is restore-eligible only when its versioned inventory is complete and exactly matches the destination topology.

**Rationale:** A struct being serializable does not establish completeness. An explicit inventory turns omissions, duplicates, and topology drift into deterministic validation errors and gives tests a finite coverage target.

### 2. Capture architecture state according to declared KVM capabilities

**Choice:** Capture vCPU events, the explicit required migratable MSR set, and the selected extended-state representation in addition to existing registers. Capture and restore use one declared extended-state mode rather than silently mixing legacy FPU and XSAVE authority. If the configured exact-snapshot profile requires a KVM surface that cannot be read or restored, capture or preflight fails closed.

**Rationale:** Pending exceptions, interrupts, syscall and timing MSRs, and SIMD/extended register files can change the next instruction's behavior even when general registers and guest memory match.

### 3. Snapshot transport and backend state by stable device identity

**Choice:** Each virtio record is keyed by stable device identity and includes base/IRQ identity, feature selectors, negotiated driver features, status, interrupt status, config generation, selected queue, and every queue's size, readiness, addresses, and cursor. Backend-specific records capture all mutable deterministic state, including block backing/fault state, network queues and shaping state, and virtio-rng generator state.

Restore matches by stable identity and backend kind; it never relies on positional `zip`. Unknown, missing, duplicate, or extra devices block restore.

**Rationale:** The next MMIO read, descriptor pop, interrupt, packet, block completion, or entropy byte is determined by these fields.

### 4. Preflight before mutation and apply in an explicit order

**Choice:** A pure validator checks schema version, capability profile, component presence, byte/range invariants, vCPU count and identity, device topology, queue count, and backend kind before restore begins. The shell then applies a documented order: quiesce execution, restore memory and in-kernel devices, restore vCPU architecture state, restore VMM/device/backend state, re-establish eventfd/irqfd bindings, and resume only after postconditions hold.

A preflight failure performs no destination mutation. An ioctl failure during the imperative phase leaves the VM non-runnable and returns a phase-specific error; it never reports restore success.

**Rationale:** Validation after partial mutation creates a state that is neither the old VM nor the snapshot.

### 5. Version fidelity separately from artifact references

**Choice:** The VM snapshot payload carries an internal state-schema version and completeness profile. Incomplete legacy payloads may be decoded for inspection or migration diagnostics, but exact restore and snapshot-backed replay reject them unless an explicit compatibility adapter proves and produces a complete current snapshot.

External snapshot references, paths, stores, and evidence DTOs remain outside this package.

**Rationale:** Payload fidelity and artifact discovery are different authorities. Keeping them separate avoids overlap with the active replay-evidence extraction work.

### 6. Test continuation, not only field serialization

**Choice:** Plain pure-core tests cover inventory and compatibility validation. Device tests mutate every owned field, snapshot, mutate again, restore, and compare canonical state. KVM integration tests checkpoint at pending-event, extended-register, queue-cursor, pending-interrupt, network, block, and entropy boundaries and compare a bounded continuation trace against an uninterrupted control run.

**Rationale:** Byte round trips can pass while capture/restore orchestration still omits or misorders live state.

## Risks / Trade-offs

- Required KVM ioctls and MSR availability vary by host. Exact mode intentionally rejects unsupported profiles rather than weakening its determinism claim.
- Snapshot payloads grow and legacy payloads may become non-restorable without migration.
- Device quiescence and restore ordering add implementation complexity; assertion-heavy state transitions and focused adapters keep that complexity reviewable.
- Continuation equivalence is bounded by the selected guest, host capability profile, and observation horizon.
