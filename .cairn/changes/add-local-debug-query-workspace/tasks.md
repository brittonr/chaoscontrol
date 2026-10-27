# Tasks

## Phase 1: Baseline and source inventory

- [x] [serial] Record current recording, dlog, assertion, fault-stage, serial, marker, and frontier formats and their existing identities. r[chaoscontrol.debug_query.event_envelope]
- [x] [serial] Confirm the existing replay debugger and persisted-session boundary so the query workspace does not duplicate exact restore. r[chaoscontrol.debug_query.moment]
- [ ] [serial] Publish the envelope adapter rules for source identity, tick, owner, ordering, duplicate rejection, and unsupported records. r[chaoscontrol.debug_query.event_envelope] r[chaoscontrol.debug_query.nonclaims]

## Phase 2: Envelope and query core

- [ ] [serial] Implement bounded adapters from retained sources into the canonical event envelope. r[chaoscontrol.debug_query.event_envelope]
- [ ] [serial] Implement the typed AST, pure evaluator, deterministic ordering, and scan/row/byte/operation accounting. r[chaoscontrol.debug_query.language]
- [ ] [parallel] Add positive query fixtures for cross-source filtering, map/contains/matches, union, windows, and up-to-moment evaluation. r[chaoscontrol.debug_query.language] r[chaoscontrol.debug_query.fixtures]
- [ ] [parallel] Add negative query fixtures for duplicate positions, malformed AST, unsupported operation, and every bound. r[chaoscontrol.debug_query.language] r[chaoscontrol.debug_query.fixtures]

## Phase 3: Moment and timeline

- [ ] [serial] Implement moment selection, past/future horizons, and exact-session admission without boot fallback. r[chaoscontrol.debug_query.moment]
- [ ] [serial] Implement the static bounded timeline and branch graph renderer from query and frontier facts. r[chaoscontrol.debug_query.timeline]
- [ ] [parallel] Add fixtures for historical windows, missing sessions, corrupt sessions, branch parentage, truncation, and graph over-bound. r[chaoscontrol.debug_query.moment] r[chaoscontrol.debug_query.timeline] r[chaoscontrol.debug_query.fixtures]

## Phase 4: Guest observation and evidence

- [ ] [serial] Implement admitted observation-command profiles, bounded execution through the guest supervisor, and counterfactual labeling. r[chaoscontrol.debug_query.guest_observation]
- [ ] [serial] Emit bounded receipts for queries, timelines, and observations with source, query/profile, moment, limit, and output identities. r[chaoscontrol.debug_query.evidence]
- [ ] [parallel] Add negative fixtures for path escape, unadmitted command, output overflow, snapshot mismatch, source drift, accounting mismatch, and overclaim. r[chaoscontrol.debug_query.guest_observation] r[chaoscontrol.debug_query.evidence] r[chaoscontrol.debug_query.nonclaims]
- [ ] [serial] Exercise one real persisted session: query the retained history, render the branch graph, and run one declared observation command. r[chaoscontrol.debug_query.moment] r[chaoscontrol.debug_query.guest_observation]

## Phase 5: Verification and evidence

- [ ] [serial] Run focused replay, explorer, evidence, VMM, and SDK tests; Clippy with warnings denied; octet; Cairn validation and gates; and relevant Nix checks. r[chaoscontrol.debug_query.fixtures]
- [ ] [serial] Record the bounded session receipt, source and command identities, positive and negative results, and remaining non-claims. r[chaoscontrol.debug_query.evidence] r[chaoscontrol.debug_query.nonclaims]
