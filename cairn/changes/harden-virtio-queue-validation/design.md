## Context

`VirtQueue::set_size` stores `size.min(max_size)`, `VirtioMmioDevice::write` narrows the MMIO value before that call, and `set_ready` accepts any configuration. `read_avail_ring` and `add_used` take modulo queue size. Descriptor/ring address calculations use unchecked addition and multiplication. `walk_descriptor_chain` detects cycles but does not validate memory ranges, aggregate length, unsupported flags, or device-specific shape. Block, net, and entropy backends allocate one vector per guest length.

Because every virtio MMIO write immediately calls `process_queues`, a malformed guest can reach these paths during configuration, not only after a valid driver handshake.

## Decisions

### 1. Model queue configuration as a pure state machine

**Choice:** Define raw MMIO queue inputs separately from `ValidatedQueueConfig`. A pure transition core checks register width, queue selection, nonzero power-of-two size within the offered maximum, required alignments, checked ring footprints, non-overlap policy, complete guest-memory containment, negotiated features, device status, and legal ready/reset transitions.

Only a validated configuration can enter ready state or be processed. Invalid writes return a typed transport violation and leave the last valid configuration authoritative or move the device to a defined needs-reset state.

**Rationale:** Individual setters cannot enforce invariants that span size, addresses, features, and status.

### 2. Validate ring progress before reading entries

**Choice:** The core computes available work with wrapping-index rules and rejects an available-index delta greater than queue capacity. Ring address calculations use checked multiply/add operations and validated footprints. The shell reads only addresses produced by the core.

A planned pop does not commit `last_avail_idx` until the request outcome defines whether the entry is completed, rejected-and-consumed, or the device requires reset.

**Rationale:** Unbounded producer deltas and early cursor mutation can cause stale reads, lost requests, or repeated malformed work.

### 3. Plan complete descriptor chains and requests

**Choice:** A pure descriptor-chain planner validates the head index, descriptor count bounded by queue size, cycle freedom, supported flags, checked `addr + len`, full guest-memory containment, checked aggregate length, and device-specific direction/order/shape. Unsupported indirect descriptors are rejected unless that feature is deliberately implemented and negotiated.

Block planning also checks header/status descriptors, sector/range arithmetic, and operation-specific directions. Net planning enforces a named frame/request budget. Entropy planning accepts only writable buffers within its named request budget.

**Rationale:** Generic cycle detection is necessary but not sufficient for memory safety or protocol correctness.

### 4. Bound all host resources independently of guest lengths

**Choice:** `VirtioLimits` contains named queue, descriptor, aggregate request, frame, block-transfer, entropy-transfer, and scratch-buffer bounds selected by device policy. No host allocation is sized solely from an untrusted descriptor length. Backends process validated requests through fixed or bounded scratch chunks; bounded metadata allocation uses checked capacity and fallible reservation.

The validation core returns a resource-limit error before backend work when aggregate limits are exceeded.

**Rationale:** A guest-controlled `u32` length can request host-scale memory even when the address is invalid or the operation would later fail.

### 5. Validate before side effects and commit once

**Choice:** Request handling follows explicit phases: snapshot raw queue facts, validate queue/ring, read bounded descriptor metadata, validate complete request, reserve bounded resources, perform guest reads, execute backend action, perform guest writes, write completion, commit cursor, then signal interrupt. Entropy PRNG advancement and block/network mutations occur only after all prerequisite guest ranges and request invariants validate.

If an imperative operation fails after a non-reversible side effect, the device enters a typed failed/needs-reset state and does not report a successful completion.

**Rationale:** Validation after PRNG advancement or backend mutation makes malformed requests alter deterministic continuation.

### 6. Define deterministic malformed-input semantics

**Choice:** Request-local protocol errors produce the device-specific error completion when a validated completion/status buffer is safely writable. Queue/transport corruption that prevents safe completion sets the virtio device-needs-reset/failure state, stops processing that queue, and emits a bounded diagnostic. No malformed input path panics, loops indefinitely, repeatedly allocates, or silently succeeds.

**Rationale:** The VMM must remain in control even when the guest is malicious or corrupted by fault injection.

### 7. Test safety properties independently of KVM

**Choice:** Pure tests and property tests generate raw queue registers, memory extents, wrapping indices, descriptor graphs, flags, addresses, lengths, and device request shapes. A bounded in-memory shell verifies no panic, no out-of-range access, no allocation beyond declared limits, and no mutation before plan acceptance. KVM smoke fixtures exercise malicious MMIO and descriptors through the production dispatch path.

**Rationale:** Most adversarial coverage should run quickly without booting a guest, while production-path smoke tests catch orchestration mistakes.

## Risks / Trade-offs

- Strict validation may expose guest-driver behavior that previously relied on truncation or clamping; diagnostics must identify the violated virtio invariant.
- Chunked block/net/entropy processing adds control flow but provides predictable memory use.
- Device-needs-reset handling must avoid repeatedly processing the same queue while remaining compatible with driver reset behavior.
- Queue state snapshot persistence is coordinated with the snapshot-completeness package rather than duplicated here.
