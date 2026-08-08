## Why

`chaoscontrol-trace` already loads a host eBPF program, attaches to KVM tracepoints, records typed events, and compares traces. It is currently suitable for debugging, not accepted evidence: ring-buffer reservation failures are silent, userspace parse/lock losses are not accounted, per-CPU sequence numbers are compared through nondeterministic cross-CPU delivery order, target identity is PID-only, runtime tracepoint layouts can rely on a build fallback, and trace logs omit exact BPF/BTF/kernel/loader/cohort and cleanup identities.

ChaosControl should harden this existing collector as a bounded KVM trace-observation rail. This work is separate from `add-kernel-bundle-validation-rail`: that package tests imported BPF Packs inside disposable guests, while this package makes ChaosControl's own host KVM trace collector honest and identity-linked.

## What Changes

- Define a typed trace capture profile binding exact BPF object/source schema, loader, kernel/BTF/tracepoint layouts, target run/process, vCPU/CPU topology, enabled events, ordering mode, bounds, retention, and non-claims. r[ebpf_trace_evidence.profile]
- Require exact runtime cohort and tracepoint-layout admission; builds produced from fallback type stubs remain debug-only until compatible runtime facts are verified. r[ebpf_trace_evidence.admission]
- Add producer attempt/submission/reservation-drop counters plus userspace received/malformed/unknown/over-bound/lock/poll accounting and reconcile them at epoch completion. r[ebpf_trace_evidence.accounting]
- Preserve CPU/source identity and sequence continuity, restrict exact event-order comparison to an admitted single-producer cohort, and use partial-order or aggregate comparison for multi-producer traces. r[ebpf_trace_evidence.ordering]
- Move parsing, canonicalization, completeness classification, cohort compatibility, and trace comparison into pure deterministic functions that reject unknown or malformed records. r[ebpf_trace_evidence.comparison]
- Bind collection to a non-reused target process/run identity, own skeleton/ring/link lifetimes safely, and record final drain, detach, unpin, and cleanup outcomes. r[ebpf_trace_evidence.lifecycle]
- Emit redacted domain-separated BLAKE3 trace and comparison receipts binding exact artifact/cohort/profile/accounting/ordering/target/cleanup identities. r[ebpf_trace_evidence.evidence]
- Add cheap pure/static conformance and a separate privileged KVM/eBPF lane with positive and negative loss, ordering, cohort, target, failure, and cleanup cases. r[ebpf_trace_evidence.verification]

## Impact

- **Collector**: `crates/chaoscontrol-trace/src/collector.rs` gains bounded accounting, exact identity, safe lifecycle ownership, and explicit terminal status.
- **BPF program/build**: `src/bpf/kvm_trace.bpf.c` and `build.rs` gain loss counters and evidence eligibility metadata; fallback compilation no longer implies runtime compatibility.
- **Pure core**: event parsing, canonicalization, completeness, ordering-mode admission, and comparison become independently testable without root, KVM, or eBPF.
- **CLI/artifacts**: trace logs and verify output become versioned identity-linked records with explicit incomplete/unsupported classes.
- **Compatibility**: debug capture may still run for explicitly marked unsupported/fallback cohorts, but cannot produce accepted trace evidence.
- **Claims**: matching complete traces support only bounded KVM trace-observation equivalence for one exact cohort and ordering mode. They do not prove VM determinism, BPF safety, kernel correctness, replay correctness, or release eligibility.
