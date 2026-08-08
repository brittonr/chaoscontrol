## Phase 1: Semantic history core

- [x] [serial] Define history v2 events, typed operation pairing, logical and attempt identities, lifecycle events, bounds, and completeness accounting. r[chaoscontrol.semantic_history.schema] r[chaoscontrol.semantic_history.outcomes]
- [x] [serial] Implement pure event admission, pairing, pending finalization, retry validation, and deterministic diagnostics. r[chaoscontrol.semantic_history.schema] r[chaoscontrol.semantic_history.outcomes] r[chaoscontrol.semantic_history.boundary]
- [x] [serial] Implement domain-separated BLAKE3 identity over the canonical semantic projection. r[chaoscontrol.semantic_history.identity]
- [x] [parallel] Add positive and negative fixtures for valid pairing, duplicate events, orphan completions, changed retry identity, pending operations, malformed values, and overflow. r[chaoscontrol.semantic_history.validation]

## Phase 2: Linearizability models and search

- [x] [serial] Define pure model interfaces plus read/write register and compare-and-swap transition models. r[chaoscontrol.semantic_history.models]
- [x] [serial] Implement real-time predecessor construction and bounded state-space search with `valid`, `invalid`, and `unknown` verdicts. r[chaoscontrol.semantic_history.linearizability]
- [x] [serial] Add canonical memoization and admitted independent-key decomposition without assuming key independence. r[chaoscontrol.semantic_history.linearizability] r[chaoscontrol.semantic_history.models]
- [x] [parallel] Add valid sequential, valid concurrent, stale-read, conflicting compare-and-swap, indefinite, pending, and bound-exhaustion fixtures. r[chaoscontrol.semantic_history.validation]
- [x] [serial] Add linearization witnesses and bounded invalid-history reduction with honest reduction status. r[chaoscontrol.semantic_history.witness]

## Phase 3: Compatibility and independent conformance

- [x] [serial] Keep a history v1 compatibility reader and deny v1 promotion into v2 linearizability evidence. r[chaoscontrol.semantic_history.compatibility]
- [x] [serial] Add import and export adapters for a pinned Jepsen-compatible history form without adding a core runtime dependency. r[chaoscontrol.semantic_history.reference_conformance]
- [x] [parallel] Add independent-oracle agreement fixtures and disagreement retention with positive and negative cases. r[chaoscontrol.semantic_history.reference_conformance] r[chaoscontrol.semantic_history.validation]

## Phase 4: Evidence and operator views

- [x] [serial] Emit bounded reports that bind model, profile, history, checker, bounds, completeness, verdict, witness, reduction, and non-claims. r[chaoscontrol.semantic_history.evidence]
- [x] [serial] Join applied and observed fault events only after `verify-fault-application-outcomes` provides accepted effect records. r[chaoscontrol.semantic_history.timeline]
- [x] [parallel] Add shared text, JSON, and static HTML semantic timelines for operations, phases, faults, latency, and witnesses. r[chaoscontrol.semantic_history.timeline]

## Phase 5: Validation and closeout

- [x] [parallel] Add malformed-history, unsupported-model, incomplete-evidence, search-bound, reference-disagreement, digest-drift, and overclaim rejection tests. r[chaoscontrol.semantic_history.validation] r[chaoscontrol.semantic_history.evidence]
- [x] [serial] Document v1 migration, outcome semantics, checker limits, reference-tool boundaries, timeline non-causation, and downstream adapter guidance. r[chaoscontrol.semantic_history.compatibility] r[chaoscontrol.semantic_history.evidence]
- [x] [serial] Run focused tests, dashboard fixtures, reference conformance, workspace checks, and Cairn proposal, design, tasks, and validation gates. r[chaoscontrol.semantic_history.validation]
