## Context

The current collector filters KVM tracepoints by target TGID, emits fixed raw records to a ring buffer, parses them into `TraceEvent`, and compares event vectors while ignoring host timestamps and per-CPU sequence numbers. The BPF program returns success when `bpf_ringbuf_reserve` fails, so missing events are invisible. Userspace ignores malformed short records and mutex poisoning. The comparison assumes ring-buffer delivery order is a deterministic semantic order even though events may be produced on multiple CPUs.

The build generates `vmlinux.h` from a selected BTF source when available and otherwise compiles against a handwritten stub plus manual tracepoint context layouts. The resulting trace log records only PID, events, kernel version text, CPU model text, and wall-clock start time. It does not bind the object, BTF, tracepoint format, loader, profile, target process lifetime, topology, accounting, or cleanup.

This package hardens that collector without changing the ownership of imported BPF Pack behavior tests. It produces a narrow host trace-observation artifact, not a proof that the VM or eBPF program is correct.

## Decisions

### 1. Define an explicit bounded capture profile

**Choice:** A typed profile binds event schema, exact BPF object and loader identities, kernel release/architecture/BTF and required tracepoint-format signatures, target run/process identity, enabled tracepoints, PID/cgroup filter semantics, vCPU and producer topology, CPU affinity where required, ordering mode, ring/queue/poll/event/artifact bounds, retention, and non-claims. Every nontrivial limit is named in profile or code constants.

**Rationale:** Capture behavior and evidence eligibility cannot depend on hidden loader defaults.

### 2. Separate buildability from runtime evidence eligibility

**Choice:** Build output records the BTF source identity and whether fallback type stubs were used. Before attach, the runtime verifies the exact running kernel/BTF identity and canonical signatures for every required KVM tracepoint layout against the compiled record projection. A fallback build may remain useful for debug compilation, but absent or mismatched runtime verification is blocked for accepted evidence.

**Rationale:** Successful ELF compilation does not prove that handwritten offsets match a running kernel.

### 3. Account for every capture stage

**Choice:** The BPF side maintains bounded per-CPU counters for eligible attempts, submitted records, and ring-reservation drops. Userspace records received records, invalid sizes, unknown discriminants, parse failures, over-bound drops, callback/lock failures, polls, poll failures, and final-drain outcomes. Epoch completion snapshots and reconciles producer and consumer accounting with checked arithmetic. Non-zero or unavailable required loss prevents completeness.

**Rationale:** A trace comparison is unsound when missing events are indistinguishable from events that never happened.

### 4. Preserve source identity and sequence continuity

**Choice:** Raw events carry CPU/source identity and a source-local sequence. Userspace adds a capture index only as delivery provenance. The pure core validates source-local monotonic continuity and duplicate/gap conditions. Host timestamps and callback order cannot define deterministic semantic order.

**Rationale:** Per-CPU sequence numbers without CPU identity are ambiguous, while global arrival order can vary under equivalent concurrent execution.

### 5. Bound exact-order comparison to an eligible topology

**Choice:** Exact event-by-event comparison is allowed only for a declared single-producer cohort that constrains vCPU/producer topology and affinity, has continuous source sequence, and completes with zero required loss. Multi-producer captures use a declared partial-order or bounded aggregate projection that compares source-local sequences and canonical window summaries; they cannot be upgraded to exact total-order evidence by sorting timestamps or arrival indices.

**Rationale:** Restricting the claim is safer than inventing a causal order unavailable from tracepoint records.

### 6. Make parsing and comparison pure and fail closed

**Choice:** Pure functions validate raw record size/version/discriminant, parse known event variants, canonicalize source-local streams and aggregate windows, classify completeness and cohort compatibility, and compare only compatible evidence modes. Unknown event types, malformed records, incompatible profiles/cohorts, incomplete accounting, or unsupported ordering modes return typed non-pass results. `Unknown` must not masquerade as a known event type.

**Rationale:** The evidence decision must be testable without a loaded BPF program and must not hide schema drift.

### 7. Bind the target beyond a reusable PID

**Choice:** The shell binds capture to an exact run identity and a stable process-lifetime handle/facts, including target TGID, process start identity, executable artifact identity where available, and expected VMM/topology profile. PID reuse, early exit, exec drift, or target mismatch terminates or invalidates the epoch explicitly. Public receipts omit raw command lines and private paths.

**Rationale:** A numeric PID alone can silently select another process.

### 8. Own and close all kernel resources

**Choice:** Collector state safely owns the open object/skeleton, maps, links, ring buffer, and target handle without leaked allocation or lifetime erasure that escapes ownership. Startup records partial open/load/attach state. Shutdown performs a bounded quiesce/final poll/accounting snapshot, detach, unpin where applicable, and cleanup classification. Drop remains best-effort safety; accepted evidence requires explicit terminal cleanup outcome.

**Rationale:** Process lifetime assumptions and implicit detach are insufficient for evidence and make failure testing difficult.

### 9. Version and hash trace artifacts

**Choice:** A canonical trace manifest and domain-separated BLAKE3 identity bind profile, BPF object, loader, kernel/BTF/layout, target, topology, ordering mode, accounting, event/aggregate artifact refs, start/end boundary facts, and cleanup. Comparisons emit a separate receipt that binds both complete compatible trace identities, comparison mode, result, first bounded divergence, and non-claims. Raw event files and verifier logs remain bounded debug artifacts, not authority.

**Rationale:** Comparing unbound JSON vectors can accidentally mix unrelated collectors and environments.

### 10. Keep validation rails distinct

**Choice:** Cheap checks cover pure parsing/accounting/comparison, schema/layout fixtures, source guards, build metadata, and claim boundaries. A dedicated privileged lane runs exact KVM/eBPF attach/capture/loss/target-exit/detach/cleanup cases. Missing root capabilities, KVM, BTF, tracepoints, or the pinned loader yields blocked evidence.

**Rationale:** Portable CI should catch most drift, while host behavior claims require actual host behavior.

## Risks / Trade-offs

- Exact runtime layout admission can block kernels that were previously debugged successfully. This is deliberate for accepted evidence; debug mode remains explicit.
- Single-producer exact mode covers fewer SMP workloads. Partial-order and aggregate modes preserve useful observations without overstating sequence equivalence.
- More counters and identities enlarge artifacts and BPF maps. Named bounds and canonical manifests keep the surface finite.
- Stable target handles and cleanup sequencing add shell complexity. Pure transition planning and explicit terminal states keep failure behavior reviewable.
- Zero observed loss cannot prove the probe set captures every determinism-relevant event. The receipt remains trace-observation evidence only.

## Non-Goals

- Replacing ChaosControl's internal deterministic log, replay receipts, snapshot evidence, or guest SDK observations.
- Validating imported Onix BPF Packs inside guests; `add-kernel-bundle-validation-rail` owns that behavior.
- Proving the BPF verifier, kernel, KVM, VMM, tracepoint implementation, or collector is correct.
- Treating matching traces as universal VM determinism, production readiness, security proof, or release eligibility.
- Supporting arbitrary host tracing, unbounded raw logs, or ambient process discovery.
