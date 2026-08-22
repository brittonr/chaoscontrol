# Design: Model guest storage flush failures

## Context

The deterministic block model already separates base, dirty, and volatile pages. The virtio boundary does not yet represent a guest flush command.

The new behavior must preserve the functional-core and imperative-shell split. Request admission, fault selection, state transitions, and oracles stay pure. KVM execution, guest memory access, disk I/O, and process control stay in the shell.

## Decisions

### Decision: Flush is a first-class block operation

Add a `Flush` operation with the exact virtio descriptor shape. A flush transfers no data and writes one status byte.

Unsupported feature negotiation or malformed descriptor chains fail before device mutation.

### Decision: Faults bind one operation attempt

Each flush attempt receives a deterministic identity and one scheduled result. Supported results are success, immediate `EIO`, delayed `EIO`, and acknowledged-without-durable-write.

Fault selection does not depend on host time or ambient randomness.

### Decision: Cache and persistence remain distinct

The model records volatile device bytes, durable device bytes, and guest cache mode as separate facts.

Restart profiles distinguish application restart with cache retained, explicit cache eviction, and full guest reboot. A successful later flush does not rewrite earlier failed clean pages unless the guest issues new writes.

### Decision: Persistent Raft uses one disk per node

Each node stores terms, votes, log entries, snapshots, and application state on its own deterministic block device. Process restart preserves only selected disk and cache state.

The workload reports protocol facts. It does not delegate commitment policy to the VMM.

### Decision: Oracles reject unsafe repair

Oracles detect acknowledged data loss, committed-entry truncation, voting after unvalidated repair, conflicting committed histories, and unsupported progress under unknown commitment.

The oracle can accept fail-closed unavailability.

### Decision: Recovery progress is participant-scoped

The workload adapter evaluates recovery only after it has the admitted participant set, disruptive-fault state, source sequences, loss counters, final drain, and finite virtual progress horizon.

If local durable state is sufficient, remote repair is an efficiency failure. If an admitted peer has the exact missing committed item, failure to repair it within a stable window is a liveness failure. Global unavailability requires complete observations that every permitted source lacks the item.

The VMM transports and groups opaque observations. The workload adapter owns these protocol decisions.

### Decision: Receipts bind the exact cohort

Every result binds candidate BLAKE3, guest image, kernel, filesystem, mount profile, virtio features, disk geometry, workload, schedule, and oracle version.

## Test Design

Positive tests cover valid flush parsing, durable flush, rewritten-page recovery, clean persistent restart, peer repair, and admitted rejoin.

Negative tests cover malformed flush chains, unsupported features, immediate and delayed errors, lying flush, stale cache, partial writes, corruption, full disk, snapshot mismatch, lost term or vote, and unsafe election.

## Claim Boundary

The campaign provides deterministic observations for one declared cohort. It does not prove arbitrary Linux, filesystem, device, Redb, Raft, or Molten behavior.
