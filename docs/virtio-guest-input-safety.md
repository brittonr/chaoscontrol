# Virtio Guest-Input Safety

ChaosControl treats each virtio MMIO value and queue value as untrusted guest input.
The transport validates a complete queue configuration before it makes the queue ready.

## Validation order

The transport uses these phases:

1. It validates the MMIO width, status transition, feature selection, and queue selection.
2. It validates the queue size, alignment, ring footprint, memory containment, and ring separation.
3. It validates the available-index delta and reads one bounded descriptor-table snapshot.
4. It validates the complete descriptor chain and the device request shape.
5. It reserves the bounded host buffer and reads all required guest data.
6. It starts backend work only after the complete request plan is valid.
7. It writes the used entry, commits the host cursor, and then publishes the interrupt.

r[impl chaoscontrol.virtio_safety.boundary] Pure functions own queue geometry, ring progress, descriptor graphs, and request plans.
The MMIO, guest-memory, allocation, backend, used-ring, and interrupt operations stay in imperative shells.

## Named limits

`VirtioLimits::default()` defines the host-owned limits.
A guest value cannot increase these limits.

| Limit | Default |
| --- | ---: |
| Queue entries | 256 |
| Descriptors in one chain | 256 |
| Aggregate descriptor bytes | 2 MiB |
| Block transfer | 1 MiB |
| Network frame | 64 KiB |
| Retained network TX packets | 256 |
| Retained network TX bytes | 4 MiB |
| Entropy transfer | 64 KiB |
| Scratch buffer | 16 KiB |
| Guest-memory regions in one validation view | 16 |

r[impl chaoscontrol.virtio_safety.resource_bounds] Block and entropy paths use bounded scratch chunks.
The network TX path uses one fallible buffer after frame validation.
It checks retained packet and byte limits before queue or statistics mutation.
Host TX helpers return each limit or allocation error to the caller.
Metadata planning uses fixed arrays that have the queue limit as their size.

## Queue and status rules

A modern Linux driver uses the accumulated status sequence `1`, `3`, `11`, and `15`.
It configures and readies queues after status `11` and before status `15`.
ChaosControl requires `VIRTIO_F_VERSION_1` and rejects unsupported negotiated feature bits.

A queue size must be nonzero, fit the queue field, and be a power of two.
The size must not exceed the offered maximum.
The transport does not truncate or clamp the MMIO value.

The descriptor table must have 16-byte alignment.
The available ring must have 2-byte alignment.
The used ring must have 4-byte alignment.
All accessed bytes must be in guest memory, and the three ranges must not overlap.

r[impl chaoscontrol.virtio_safety.queue_configuration] Only `ValidatedQueueConfig` can become ready.
r[impl chaoscontrol.virtio_safety.ring_progress] Wrapping available-index deltas larger than the queue capacity fail before ring iteration.

## Descriptor and request rules

ChaosControl rejects descriptor cycles, bad head or next indices, and excessive descriptor counts.
It also rejects unknown flags, indirect descriptors, range overflow, memory holes, and excessive aggregate lengths.

Block requests require a readable header, correctly directed data buffers, and a writable status byte.
The storage offset and end use checked sector arithmetic.
Network requests require a complete 10-byte virtio header and the correct queue direction.
Entropy requests accept only writable buffers.

r[impl chaoscontrol.virtio_safety.descriptor_validation] The shell reads only descriptor addresses from the validated plan.
r[impl chaoscontrol.virtio_safety.request_validation] A backend receives only a complete device request plan.

## Failure and reset behavior

A safe block request error writes `VIRTIO_BLK_S_IOERR` through an independently validated status byte.
This error completion consumes one request but does not report successful block I/O.

Queue or transport corruption sets `VIRTIO_STATUS_DEVICE_NEEDS_RESET` and stops that queue.
The device does not write a used entry or publish an interrupt for this failure.
A status write of zero resets the transport and clears the failed queue state.

Allocation and backend failures keep the host cursor unchanged.
The live state retains one bounded typed request outcome after a safe block error completion.
A later valid request can replace that outcome without queue poisoning.
If effects started, the live state retains the pending completion and its `effects_started` flag.
It also records whether backend work started.
A used-ring failure keeps this pending state and requires a reset.
The used index is the completion authority if a used element was only partly written.
KVM interrupt assertion or deassertion failure poisons the device and returns a typed VM error.

r[impl chaoscontrol.virtio_safety.mutation_order] Cursor commit occurs only after the used index write succeeds.
r[impl chaoscontrol.virtio_safety.failure_semantics] Malformed requests cannot silently retry after the queue enters its failed state.

## Snapshot boundary

`VirtioMmioDevice::live_state()` exposes the validated addresses, queue size, cursors, typed failure, and pending completion.
This view lets the separate snapshot owner capture validated live state.
This change does not add a snapshot payload, codec, persistence path, or restore owner.

r[impl chaoscontrol.virtio_safety.snapshot_state] The virtio owner exposes state but does not own snapshot serialization.

## Validation scope

The fast corpus covers full-width registers, wrapping indices, descriptor graphs, flags, addresses, lengths, and request shapes.
Production-path tests cover block, network, entropy, allocation failure, backend failure, and mutation order.
A bounded KVM test runs a malicious guest MMIO write through `VirtioMmioDevice::write_at`.

Run the focused rails:

```bash
nix develop -c cargo test -p chaoscontrol-vmm \
  --test virtio_transport \
  --test virtio_block_paths \
  --test virtio_net_entropy_paths \
  --test virtio_net_retention \
  --test virtio_post_progress \
  --test virtio_validation \
  --test virtio_properties

nix develop -c cargo test -p chaoscontrol-vmm \
  --test virtio_kvm_smoke -- --ignored

nix build .#checks.x86_64-linux.virtio-malicious-guest-kvm-smoke --no-link -L
```

r[verify chaoscontrol.virtio_safety.validation] The focused, workspace, KVM, and Nix rails cover the declared validation scope.
r[verify chaoscontrol.virtio_safety.validation.core] The pure validation tests cover valid and malformed queue, descriptor, progress, and request inputs.
r[verify chaoscontrol.virtio_safety.validation.positive] The production-path tests include valid block, network, and entropy requests.
r[verify chaoscontrol.virtio_safety.validation.negative] The negative tests assert no early cursor, backend, entropy, used-ring, or interrupt change.
They also cover block, network TX, network RX, entropy, used-ring, and KVM interrupt failures after effects start.
r[verify chaoscontrol.virtio_safety.validation.fuzz] The generated corpus has a no-panic and policy-bound oracle.
r[verify chaoscontrol.virtio_safety.validation.kvm] The KVM smoke requires a typed needs-reset failure and a later guest halt.

These tests cover the implemented modern MMIO block, network, and entropy devices.
They do not prove support for indirect descriptors, packed rings, PCI transport, or other device classes.
They do not prove complete virtio conformance for all Linux versions or guest drivers.
