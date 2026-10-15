# Evaluate Page-Addressed Snapshots

## Why

Explorer branching and retained replay-parent snapshots are concrete consumers of snapshot storage. Today `SnapshotMemory::Overlay` shares a base through `Arc` in memory, but its Serde representation embeds a cloned base plus dirty-page bytes. `FileSnapshotStore` serializes the complete `SimulationSnapshot` envelope as CBOR, compresses it with zstd, and identifies the compressed artifact with SHA-256. Shared in-memory pages therefore do not imply persistent cross-snapshot deduplication. Neither this observation nor another engine's design establishes a storage or latency improvement.

The immediate outcome is a finite, reproducible evaluation and an `adopt`, `defer`, or `reject` decision receipt, not a production store migration. The durable capability is a correctness-first, bounded method for comparing snapshot representations without weakening exact replay. The VMM snapshot maintainers own fidelity admission; explorer/replay-evidence maintainers own the evaluation harness, retained evidence, and recommendation. The maintenance owner must be named in each evaluation receipt.

## What Changes

- Specify an isolated Rust local-machine experiment comparing the existing whole-envelope CBOR+zstd artifact and Arc-overlay paths with persistent 4 KiB BLAKE3 page objects and a bounded keyframe DAG. Keep baseline codecs and public replay references unchanged. r[chaoscontrol.snapshot_page_evaluation.scope]
- Predeclare Nickel-backed profiles, finite resource budgets, workload cases, ordering, costs to measure, and decision criteria. Cover sparse, dense, identical-page, sibling, deep, multi-VM, inherited-page, host-write, and non-memory-state cases. Admit the retained Raft and redb cohorts only when their exact source, artifact, completeness, and replay inputs validate. r[chaoscontrol.snapshot_page_evaluation.protocol]
- Require independent full-snapshot materialization and bounded continuation equivalence before interpreting any paired metrics. Known deep-lineage gaps or invalid baselines block comparison and adoption; do not optimize corrupted state. r[chaoscontrol.snapshot_page_evaluation.fidelity]
- Evaluate bounded reference validation, corruption/missing objects, unsafe locators, cycles/depth, interrupted publication, root pinning, and conservative reachability/GC. Published proof artifacts are never deletion candidates. r[chaoscontrol.snapshot_page_evaluation.references] r[chaoscontrol.snapshot_page_evaluation.retention]
- Retain exact identities, commands, raw observations, retained bytes and dedup accounting, capture/restore/seek distributions, chain depth, and CPU/I/O/memory/metadata costs. No measured speedup or storage-saving claim exists in this planning change. r[chaoscontrol.snapshot_page_evaluation.measurements]
- Conclude with a reviewable recommendation and explicit blockers; no-go/defer is a complete valid evaluation outcome. Adoption means recommending a separate reviewed production codec/reference migration, never enabling it here. r[chaoscontrol.snapshot_page_evaluation.decision]

## Impact

Only this active native Cairn package changes now: proposal, design, delta specification, and linked tasks. Only observed source-review/foundation work may be checked. Experimental implementation, fixtures, measurements, KVM continuation smoke, decision production, and release closeout remain unchecked.

The future evaluation uses private scratch artifacts alongside, not instead of, pinned production inputs. Positive verification must demonstrate exact full-state round trips, sibling isolation, bounded equivalent continuation, valid root retention, and reproducible metric/decision derivation. Negative verification must reject incomplete/corrupt baselines, missing or forged refs, unsupported schemas, excessive bounds, unsafe paths, cycle/depth violations, partial publication, and uncertain deletion plans before evidence consumption. Wall-clock timing is measurement only and cannot influence guest scheduling or keyframe placement.

`repair-incremental-snapshot-lineage` owns overlay completeness/revert repair; this evaluation consumes its validated outcome where needed and otherwise records blocked/defer. `externalize-large-dogfood-artifacts` owns storage-neutral/remote materialization and adapter selection; local page dedup is neither that migration nor durability/trust evidence. Existing exact replay and retained snapshot stores suffice as inputs; no hard dependency on sibling debugger, witness, or PMU proposals is introduced. General attribution/interleaving minimization remains with `causality-analysis-engine`, reusable reduction with `extract-failure-reducer-core`.

## References and Licensing

The immutable [RewindVM revision](https://github.com/fzakaria/rewindvm/tree/6ec1f68de82ae3e657cdfcbe0d83f349bb367082), especially [rewind-store](https://github.com/fzakaria/rewindvm/blob/6ec1f68de82ae3e657cdfcbe0d83f349bb367082/crates/rewind-store/src/lib.rs) and [rewind-core/keyframes](https://github.com/fzakaria/rewindvm/blob/6ec1f68de82ae3e657cdfcbe0d83f349bb367082/crates/rewind-core/src/keyframes.rs), is design inspiration only, not a runtime dependency or parity proof. Engine code is MIT; the guest kernel patch is GPL-2.0; the desktop app is proprietary. Do not import app code or adopt its product scope. ChaosControl retains its own exact state inventory and deterministic progress authority.

## Out of Scope

No runtime implementation in this spec-only delivery; no production codec/store/reference cutover, proof promotion, accepted-spec sync, archive, implementation approval, remote adapter choice, hosted service, containers, non-Rust guest intake, kernel-patch import, or weaker-determinism fallback. Lifecycle gates prove structural planning facts only. Source review is not benchmark, correctness, KVM, durability, or replay evidence.
