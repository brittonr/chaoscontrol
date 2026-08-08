# Bounded eBPF KVM trace evidence

ChaosControl can collect bounded host-side observations from KVM tracepoints.
A matching trace result is valid only inside one admitted capture profile.
It is not a proof of VM determinism or replay correctness.

## Evidence boundary

The capture profile is a Nickel-authored review boundary:

- `contracts/evidence/ebpf-trace-capture-profile.ncl`
- `contracts/evidence/examples/kvm-ebpf-trace-capture-profile.ncl`

Rust owns raw records, producer counters, parsed events, manifests, and receipts.
The pure evidence core is in `crates/chaoscontrol-trace/src/evidence.rs`.
The collector shell is in `crates/chaoscontrol-trace/src/collector.rs`.

The profile binds these facts:

- BPF object, source, event schema, loader, and BTF source identities
- Expected kernel release, architecture, BTF identity, and tracepoint layouts
- Run identity, TGID, process-start identity, executable, and VMM profile
- vCPU count, producer count, and CPU affinity
- Enabled event types and filter semantics
- Ordering mode and all capture bounds
- Retention policy and required non-claims

All stack-owned identities use BLAKE3.
The compiled ring buffer has an exact size of 16 MiB.
A profile cannot declare a different ring size.

## Admission

Evidence admission compares the observed runtime cohort with the profile.
The comparison includes each required tracepoint field name, type, offset, and size.
Use `TraceCollector::for_admitted_profile` for an evidence-eligible attach.
It observes the running kernel, BTF, tracefs layouts, compiled object, and loader first.
It rejects any mismatch before it loads or attaches the BPF program.

Fallback `vmlinux.h` stubs keep sandboxed builds usable for debug work.
A fallback build is always `debug-only` and cannot produce admissible evidence.
A BTF mismatch, object mismatch, loader mismatch, or layout mismatch blocks admission.

`BuildIdentity::compiled` exposes the identities from the actual build output.
The build script hashes the generated BPF object and its exact input sources.

## Record schema and accounting

Raw schema version 2 is a 64-byte record.
Each record contains these provenance fields:

- schema version and record size
- source CPU
- source-local sequence number
- userspace callback capture index
- target TGID

The loader writes the profile event set into an explicit BPF enable map.
Disabled tracepoints do not enter producer accounting or the ring buffer.
Evidence validation also rejects retained events that were not enabled.

Unknown event discriminants remain diagnostic unknown events at the decode layer.
Evidence parsing rejects them before they can enter an accepted event stream.
Malformed sizes and wrong versions are also rejected and counted.

The BPF program retains per-CPU counters for:

1. Eligible attempts
2. Submitted records
3. Ring-buffer reservation drops

Userspace retains received, accepted, rejected, over-bound, poll, lock, and drain counts.
A complete capture must satisfy all of these conditions:

- eligible attempts equal submissions plus reservation drops
- reservation drops equal zero
- submissions equal userspace receipts
- receipts equal accepted records
- all rejection and failure counts equal zero
- event and poll counts stay inside the profile bounds
- the final drain succeeds
- accepted accounting equals the retained event count
- source-local sequences have no duplicates or gaps

Missing producer counters produce `unsupported`.
Any loss, mismatch, truncation, overflow, or failed drain produces `partial`.
Neither status can become a passing comparison.

## Ordering modes

Do not use callback order or host timestamps as semantic order.

`exact-single-producer` requires one producer and one declared affinity CPU.
It compares the source-local event sequence.

`source-partial-order` compares each source-local sequence independently.
It does not invent an order between CPUs.

`aggregate` compares bounded source counts, event-type counts, and fixed windows.
It does not claim event-by-event equivalence.

A multi-producer profile cannot select `exact-single-producer`.
Different profile identities produce `incompatible`, not `match`.
Incomplete accounting produces `incomplete`, not `match`.
Cleanup failure produces `cleanup-failed`, not `match`.

## Target identity and lifecycle

A PID is not a stable process identity.
`StableTargetHandle` owns a `pidfd` for the original process lifetime.
It reads bounded process-start, executable, PID namespace, and cgroup facts.
The profile also binds the run and VMM profile identities.
Exit, exec, PID reuse, or any identity drift makes the capture partial or blocked.

The owned collector worker keeps all libbpf objects in one thread.
It does not leak an `OpenObject` or erase a lifetime with `transmute`.
Shutdown uses this order:

1. Quiesce new target submissions.
2. Perform the final ring drain.
3. Snapshot producer accounting.
4. Drop the ring consumer and detach links.
5. Record unpin and cleanup outcomes.

`stop()` is idempotent.
`Drop` requests the same bounded shutdown if the caller did not call `stop()`.
A manifest cannot claim `complete` when accounting or cleanup is incomplete.

## Checks

Run the focused tests and static fixture checker:

```console
cargo test -p chaoscontrol-trace --all-targets
cargo run -q -p chaoscontrol-trace --bin ebpf-trace-evidence-selftest
cargo run -q -p chaoscontrol-evidence --bin check-evidence-contracts -- --root .
```

Run the privileged attachment smoke against an existing VMM TGID:

```console
sudo cargo run -q -p chaoscontrol-trace \
  --bin ebpf-trace-evidence-selftest -- \
  --privileged-smoke-pid <TGID> --require-privileged

# Generate real KVM exits and IRQ-line traffic in the same target process.
sudo cargo test -p chaoscontrol-trace \
  --test ebpf_kvm_smoke -- --ignored --nocapture
```

Without the required privilege, KVM device, BTF, or tracepoints, the smoke prints `unsupported`.
The strict option changes this result to a failing exit status.
The smoke checks attachment, bounded collection, reconciliation, drain, and cleanup.
It does not prove that the target VM is deterministic.

## Required non-claims

Every profile, manifest, and comparison receipt retains these statements:

- not VM determinism proof
- not replay correctness proof
- not eBPF safety proof
- not kernel correctness proof
- not security proof
- not physical readiness proof
- not release eligibility

Use the typed CLI after you retain canonical profile, manifest, and event JSON:

```console
chaoscontrol-trace evidence-check \
  --profile profile.json --manifest trace-a.json --events events-a.json
chaoscontrol-trace evidence-verify \
  --profile profile.json \
  --manifest-a trace-a.json --events-a events-a.json \
  --manifest-b trace-b.json --events-b events-b.json \
  --output comparison.json
```

The commands print `complete`, `partial`, `unsupported`, `incompatible`,
`divergent`, `blocked`, or `cleanup-failed` as applicable.
Only `complete` capture checks and `match` comparisons have a successful exit.

Legacy `TraceLog` and `DeterminismVerifier` APIs remain for debug compatibility.
They do not check cohort identity, accounting completeness, or cleanup.
Their output cannot be promoted into bounded eBPF evidence.
