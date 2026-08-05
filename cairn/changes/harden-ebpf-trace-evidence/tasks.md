## Phase 1: Profile, identities, and pure core

- [ ] [serial] Define the versioned capture profile, trace manifest, accounting, ordering modes, terminal classes, comparison receipt, named bounds, and non-claims. r[ebpf_trace_evidence.profile] r[ebpf_trace_evidence.evidence]
- [ ] [serial] Record exact BPF object/source schema, loader, BTF source/fallback, kernel, tracepoint-layout, target run/process, topology, and filter identities. r[ebpf_trace_evidence.profile] r[ebpf_trace_evidence.admission]
- [ ] [serial] Implement pure raw-record validation/parsing, source-sequence checks, accounting reconciliation, completeness classification, cohort compatibility, canonical partial-order/aggregate projection, and mode-aware comparison. r[ebpf_trace_evidence.accounting] r[ebpf_trace_evidence.ordering] r[ebpf_trace_evidence.comparison]
- [ ] [parallel] Add positive complete single-producer exact-order and multi-producer aggregate fixtures. r[ebpf_trace_evidence.verification]
- [ ] [parallel] Add negative malformed size/version/discriminant, unknown event, duplicate/gap, counter overflow/mismatch, non-zero/unavailable loss, incompatible cohort/profile/mode, and timestamp-sort fixtures. r[ebpf_trace_evidence.verification]

## Phase 2: BPF producer and runtime admission

- [ ] [serial] Add per-CPU eligible-attempt, submitted-record, and ring-reservation-drop accounting to the BPF program with checked userspace reconciliation. r[ebpf_trace_evidence.accounting]
- [ ] [serial] Add CPU/source identity and source-local sequence to the versioned raw event schema without treating delivery index as semantic order. r[ebpf_trace_evidence.ordering]
- [ ] [serial] Emit build metadata for exact BTF input and fallback use, then verify running kernel/BTF and every required KVM tracepoint-format signature before evidence-eligible attach. r[ebpf_trace_evidence.admission]
- [ ] [parallel] Add positive exact-layout fixtures and negative absent BTF, fallback-only, kernel drift, missing tracepoint, field offset/size/type drift, object drift, and loader drift fixtures. r[ebpf_trace_evidence.verification]

## Phase 3: Collector lifecycle and target binding

- [ ] [serial] Replace leaked/transmuted lifetime ownership with explicit collector-owned object, skeleton, map, link, ring-buffer, and target-handle state. r[ebpf_trace_evidence.lifecycle]
- [ ] [serial] Bind capture to exact run, process-start, executable artifact, VMM profile, vCPU topology, filter, and affinity facts and reject PID reuse, exec drift, or target mismatch. r[ebpf_trace_evidence.lifecycle]
- [ ] [serial] Account userspace receive, malformed, unknown, parse, over-bound, callback/lock, poll, and final-drain outcomes. r[ebpf_trace_evidence.accounting]
- [ ] [serial] Implement bounded startup, partial-attach rollback, quiesce, final poll, accounting snapshot, detach, unpin, cleanup, and explicit terminal status. r[ebpf_trace_evidence.lifecycle]
- [ ] [parallel] Add negative permission, verifier, map update, partial attach, poll, target exit/reuse, bound, cancellation, detach, unpin, and cleanup failure tests. r[ebpf_trace_evidence.verification]

## Phase 4: Evidence surfaces and rails

- [ ] [serial] Emit canonical domain-separated BLAKE3 trace manifests and comparison receipts binding exact cohort, target, profile, accounting, ordering, artifacts, divergence, and cleanup. r[ebpf_trace_evidence.evidence]
- [ ] [serial] Update CLI capture/verify output to distinguish complete, partial, unsupported, incompatible, divergent, blocked, and cleanup-failed results without raw-log verdicts. r[ebpf_trace_evidence.comparison] r[ebpf_trace_evidence.evidence]
- [ ] [parallel] Add guards preventing debug/fallback/incomplete/multi-producer-total-order traces from satisfying exact trace evidence, VM determinism, replay, BPF safety, kernel correctness, security, or release gates. r[ebpf_trace_evidence.evidence]
- [ ] [parallel] Add the cheap pure/schema/layout/source-guard rail and the separate privileged KVM/eBPF behavior rail; missing prerequisites must report blocked. r[ebpf_trace_evidence.verification]
- [ ] [parallel] Document evidence-eligible cohorts, debug mode, ordering limits, loss accounting, target binding, reproduction, cleanup, retention, and non-claims. r[ebpf_trace_evidence.evidence]
- [ ] [serial] Run focused pure-core, BPF build, collector, CLI, evidence, formatting, clippy, and privileged positive/negative checks. r[ebpf_trace_evidence.verification]
- [ ] [serial] Run Cairn validation and proposal/design/tasks gates; sync and archive only with current zero-loss exact-cohort capture, divergence, loss, incompatibility, target-exit, and cleanup evidence. r[ebpf_trace_evidence.verification]
