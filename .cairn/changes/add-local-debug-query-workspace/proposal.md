# Proposal: Local debug query workspace

## Why

ChaosControl retains more debugging facts than its operator surfaces can query. Recordings carry ordered events, dlogs carry per-exit records, bug reports bind assertions and snapshots, fault evidence carries a stage ledger, and exploration carries frontier and marker history. Each has a separate CLI or file format. There is no single bounded way to ask "which assertion, fault, and process events occurred around this moment?" or "what did the failing branch and its control differ on before the failure?"

Antithesis exposes event-set queries, a multiverse-style timeline, and a debug-moment environment. ChaosControl's product scope is local and evidence-first, so the equivalent must be a typed query engine over retained artifacts, a static timeline renderer, and a bounded declared guest-observation command path. It must not become a hosted UI or an arbitrary shell.

## What Changes

- Define one canonical bounded event envelope over recordings, dlogs, SDK events, assertion records, fault stages, serial output, and branch markers. r[chaoscontrol.debug_query.event_envelope]
- Add a typed bounded query language over event envelopes with filter, map, contains, matches, union, window, and up-to-moment operations, evaluated by a pure core. r[chaoscontrol.debug_query.language]
- Query relative to a selected retained moment or tick, with explicit past/future horizons and an exact-session requirement when a future requires restore. r[chaoscontrol.debug_query.moment]
- Render a static timeline or branch graph from frontier, checkpoint, marker, fault, and query facts without a server. r[chaoscontrol.debug_query.timeline]
- Add a bounded, declared guest-observation command path that runs an admitted command in a restored guest at a selected moment, with finite tick and output bounds and no ambient host shell. r[chaoscontrol.debug_query.guest_observation]
- Bind every query result, timeline, and observation command into a validation receipt. r[chaoscontrol.debug_query.evidence]
- Keep hosted services, causal claims, complete event capture, arbitrary shells, and remote execution out of the claim. r[chaoscontrol.debug_query.nonclaims]
- Add positive and negative fixtures for queries, moments, timeline rendering, observation commands, missing artifacts, bounds, and overclaim. r[chaoscontrol.debug_query.fixtures]

## Impact

- **Core**: event-envelope admission, query AST validation, deterministic evaluation, result accounting, and timeline layout remain pure and bounded.
- **Shells**: bounded artifact loading, dlog/recording joins, exact-session restore, guest observation execution, and static rendering.
- **Existing owners**: `chaoscontrol-replay` keeps exact sessions and continuation; `chaoscontrol-explore` keeps frontier records; `chaoscontrol-evidence` keeps receipts and evidence classes.
- **Consumers**: local operators and contributor workflows gain one query surface instead of scraping multiple formats.

## Lifecycle Prerequisites

None for historical queries over existing recordings and dlogs. A future-of-moment query or guest observation that requires restored state depends on `add-persisted-debugger-replay` for the exact session; when that session is absent, the query must reject rather than boot a default VM.

## Out of Scope

- Hosted web UI, shared query service, authentication, remote execution, or multi-user state.
- Arbitrary JavaScript, Python, shell, SQL, or network-backed query execution.
- An interactive GDB protocol, source-level debugger, or host filesystem shell.
- Causal analysis, complete-log capture, correctness, replay promotion, or release eligibility.
- Non-Rust guests, containers, or cross-machine artifacts.

## Affected Specs

- `debug-query-workspace`: event envelope, query language, moment selection, timeline, guest observation, evidence, non-claims, and fixtures.
