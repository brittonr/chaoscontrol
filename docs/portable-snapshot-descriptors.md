# Portable exact-snapshot descriptors

<!-- r[impl chaoscontrol.snapshot_descriptor.contract] -->
<!-- r[impl chaoscontrol.snapshot_descriptor.complete_cohort] -->
<!-- r[impl chaoscontrol.snapshot_descriptor.locator_boundary] -->
<!-- r[impl chaoscontrol.snapshot_descriptor.closure] -->
<!-- r[impl chaoscontrol.snapshot_descriptor.preflight] -->
<!-- r[impl chaoscontrol.snapshot_descriptor.restore_receipt] -->
<!-- r[impl chaoscontrol.snapshot_descriptor.projection] -->
<!-- r[impl chaoscontrol.snapshot_descriptor.consumer_contract] -->

ChaosControl publishes `chaoscontrol-snapshot-descriptor-v1` for external consumers.
The descriptor identifies one existing snapshot payload and its exact restore cohort.
It does not define a new snapshot payload codec or storage system.

The pure contract is in `crates/chaoscontrol-snapshot-descriptor/`.
The filesystem adapter is in `crates/chaoscontrol-evidence/src/snapshot_descriptor/`.
The VMM remains the owner of snapshot capture and restore effects.

## Versions

The public descriptor version is `1`.
The supported completeness profile is `exact-x86-kvm-v1`.
The supported VMM state schema is `2`.
The current payload codec is `simulation-snapshot-cbor-zstd-v2`.

A reader must reject unknown descriptor versions, profiles, state schemas, architectures, algorithms, and codecs.
It must not fill missing state with defaults.

## Exact cohort inventory

The descriptor binds these facts:

- the `x86_64` architecture;
- the complete KVM operation cohort;
- the sorted `KVM_GET_MSR_INDEX_LIST` inventory;
- the runtime build identity;
- the vCPU count and guest-memory size;
- each stable virtio MMIO address, IRQ, device ID, queue count, and backend class;
- the scheduler, virtual-time, and entropy profiles;
- the kernel, initrd, disk, and guest-binary artifact identities that apply;
- the snapshot payload closure.

The state-owner inventory maps the existing snapshot boundary as follows:

| Existing snapshot state | Public owner identity |
| --- | --- |
| Guest memory | `guest-memory` |
| PIC, IOAPIC, PIT, and KVM clock | `in-kernel-irqchip`, `in-kernel-pit`, `in-kernel-clock` |
| vCPU registers, special registers, FPU, debug, LAPIC, MP state, XCRs, XSAVE, events, and MSRs | `vcpu:<id>:architecture`, `events`, `msrs`, and `xsave` |
| Virtual TSC, counters, panic detector, coverage, and timer state | `deterministic-time`, `counters`, `panic-detector`, `coverage`, and `timer` |
| Serial, fault engine, entropy, and scheduler | `serial`, `fault-engine`, `entropy`, and `scheduler` |
| Virtio transport, queues, and backend | stable `virtio:<identity>:...` owners |

The adapter derives this inventory from a validated `SnapshotMetadata` value.
A missing, duplicate, reordered, or unsupported owner blocks descriptor publication.

## Canonical identity

Descriptor identity uses domain-separated BLAKE3 framing.
Every field has a tag and an explicit byte length.
Inventories must use their deterministic order before identity calculation.

JSON is a public machine projection.
Whitespace, indentation, and object formatting do not define descriptor identity.
Changing any behavior-relevant descriptor field changes the BLAKE3 identity.

The checked projection files are:

- `contracts/evidence/schema/snapshot-descriptor-v1.schema.json`;
- `contracts/evidence/snapshot-descriptor.ncl`;
- `contracts/evidence/fixtures/valid/snapshot-descriptor.valid.json`;
- `contracts/evidence/snapshot-descriptor.freshness.json`.

The freshness manifest binds the Rust field owners, JSON schema, Nickel contract, example, and generated fixture.

## Payload closure

A monolithic closure contains one payload member.
The member identity and length must equal the logical payload identity and length.

A chunked closure contains an ordered, gap-free list of chunk members.
Each chunk has an algorithm tag, digest, exact length, codec, role, and order.
The descriptor also binds the chunk-manifest identity and the logical payload identity.

The current replay store uses SHA-256 for snapshot and chunk interoperability identities.
The descriptor itself always uses BLAKE3.
Algorithm tags prevent one digest from being interpreted as another algorithm.

The shell reads every declared member within an explicit byte bound.
It rejects missing, reordered, truncated, oversized, or digest-mismatched members.

## Locator boundary

Paths, Redb keys, Iroh tickets, URLs, mirrors, and provider handles are not descriptor fields.
They can appear only in a detached `LocatorSidecar`.

Changing a locator does not change descriptor identity.
A locator does not prove that bytes exist, match a digest, remain retained, or can be read.
Consumers must verify content independently.

## Destination preflight

Preflight is a pure comparison over a descriptor and supplied destination observations.
It performs no file, KVM, memory, device, clock, network, or process operation.

Preflight compares the profile, schema, architecture, runtime build, KVM operations, scheduler, time, entropy, vCPU topology, memory shape, MSRs, devices, backends, and available memory.
Any mismatch returns ordered blockers and no restore plan.

An admitted plan lists the required restore phases.
Admission means only that the supplied facts match.
It does not prove that later materialization or restore will succeed.

## Restore observations

A detached restore receipt records:

1. descriptor and destination identities;
2. preflight identity;
3. materialization status;
4. mutation start;
5. ordered phase results;
6. poison state;
7. completion state;
8. bounded continuation observations.

A failure after mutation starts must mark the destination as poisoned.
A poisoned receipt cannot report completion.
A complete receipt requires every phase to succeed and requires a bounded matching continuation observation.

Host file descriptors, timers, performance counters, writers, and output sinks are not snapshot content.
The restore shell creates fresh handles after deterministic state admission.

## Consumer fixture

Run this command to create monolithic and chunked examples:

```bash
nix develop -c cargo run -p chaoscontrol-evidence \
  --bin snapshot-descriptor-fixture -- \
  --out target/snapshot-descriptor-fixture
```

The bundle includes a refs-only Molten-shaped consumer record.
That record contains descriptor, payload, closure, and preflight identities only.
It has no direct Molten dependency.
It rejects restore-authority, world-branch, world-merge, promotion, and release claims.

Check contract freshness with:

```bash
nix develop -c cargo run -p chaoscontrol-evidence \
  --bin check-snapshot-descriptor-contracts -- \
  --root . --check
```

## Non-claims

A valid descriptor is not proof of KVM, guest, kernel, device, storage, or replay correctness.
It does not grant read, transfer, retention, restore, execution, branch, promotion, or release authority.
It does not support cross-architecture, cross-topology, or silent cross-cohort restore.
It does not establish Molten world-commit identity or semantic merge behavior.
