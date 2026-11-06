# Tasks

## Phase 1: Baseline and source inventory

- [x] [serial] Record current recording, dlog, assertion, fault-stage, serial, marker, and frontier formats and their existing identities. r[chaoscontrol.debug_query.event_envelope]
- [x] [serial] Confirm the existing replay debugger and persisted-session boundary so the query workspace does not duplicate exact restore. r[chaoscontrol.debug_query.moment]
- [x] [serial] Publish the envelope adapter rules for source identity, tick, owner, ordering, duplicate rejection, and unsupported records. r[chaoscontrol.debug_query.event_envelope] r[chaoscontrol.debug_query.nonclaims]

## Phase 2: Envelope and query core

- [x] [serial] Implement bounded recording and dlog adapters into the canonical event envelope. r[chaoscontrol.debug_query.event_envelope]
- [x] [serial] Add bounded branch-marker and remaining fault-stage adapters. r[chaoscontrol.debug_query.event_envelope]
- [x] [serial] Implement the typed AST, pure evaluator, deterministic ordering, and scan/row/byte/operation accounting. r[chaoscontrol.debug_query.language]
- [x] [parallel] Add positive query fixtures for cross-source filtering, map/contains/matches, union, windows, and up-to-moment evaluation. r[chaoscontrol.debug_query.language] r[chaoscontrol.debug_query.fixtures]
- [x] [parallel] Add negative query fixtures for duplicate positions, malformed AST, unsupported operation, and every bound. r[chaoscontrol.debug_query.language] r[chaoscontrol.debug_query.fixtures]

## Phase 3: Moment and timeline

- [x] [serial] Implement moment selection, past/future horizons, and exact-session admission without boot fallback. r[chaoscontrol.debug_query.moment]
- [x] [serial] Implement the static bounded timeline and branch graph renderer from query and frontier facts. r[chaoscontrol.debug_query.timeline]
- [x] [parallel] Add fixtures for historical windows, missing sessions, corrupt sessions, branch parentage, truncation, and graph over-bound. r[chaoscontrol.debug_query.moment] r[chaoscontrol.debug_query.timeline] r[chaoscontrol.debug_query.fixtures]

## Phase 4: Guest observation and evidence

- [x] [serial] Implement admitted observation-command profiles, bounded execution through the guest supervisor, and counterfactual labeling. r[chaoscontrol.debug_query.guest_observation]
- [x] [serial] Emit bounded receipts for queries, timelines, and observations with source, query/profile, moment, limit, and output identities. r[chaoscontrol.debug_query.evidence]
- [x] [parallel] Add negative fixtures for path escape, unadmitted command, output overflow, snapshot mismatch, source drift, accounting mismatch, and overclaim. r[chaoscontrol.debug_query.guest_observation] r[chaoscontrol.debug_query.evidence] r[chaoscontrol.debug_query.nonclaims]
- [x] [serial] Exercise one real persisted session: query the retained history, render the branch graph, and run one declared observation command. r[chaoscontrol.debug_query.moment] r[chaoscontrol.debug_query.guest_observation]

## Phase 5: Verification and evidence

- [x] [serial] Run focused replay, explorer, evidence, VMM, and SDK tests; Clippy with warnings denied; octet; Cairn validation and gates; and relevant Nix checks. r[chaoscontrol.debug_query.fixtures]
- [x] [serial] Record the bounded session receipt, source and command identities, positive and negative results, and remaining non-claims. r[chaoscontrol.debug_query.evidence] r[chaoscontrol.debug_query.nonclaims]

Evidence: `evidence/2026-11-06-workspace-proof.ncl` exports successfully through pinned Nickel and imports the retained Rust-emitted query, observation, Markdown, and DOT receipts. Real KVM capture used seed 42, one 128 MiB VM, quantum 100, 40 retained post-setup ticks, and checkpoints at 1190/1210. The query returned 369 serial and five fault-stage rows; rerun result and receipt bytes matched. Both static formats validated. The restored observation returned the expected 10-byte `committed\n` digest in one round and validated as complete and counterfactual.

Verification execution is complete, not all-green repository readiness: the final affected matrix reports 2726 passed, six failed, and 16 ignored across 196 targets. Both failing targets are the separate assertion-summary producer/consumer mismatch (`distinct_failing_assertions`/`liveness_gaps`). Final Octet reports 395 warnings and one `explicit_defaults` error in `chaoscontrol-smr/src/storage_schedule.rs:306`. Clippy with warnings denied, Rust/Nix formatting, the two-positive/six-negative Nickel fixture check, and Nix check-graph evaluation pass. These unrelated failures are retained, not suppressed or promoted. No archive, replay, complete-capture, assertion-quality, public-build, or release claim follows from the checked task boxes.

Native validation and proposal/design/tasks gates pass. The sync plan is blocked by `change_dependency.unsatisfied_dependency: dependency add-persisted-debugger-replay is not archived`. No sync or archive was executed and no dependency was removed. Accepted-spec-only traceability retains five dangling new requirement IDs until that prerequisite permits sync. The prerequisite's broader explorer producer, replay/inspect navigation cutover, shared-store ownership migration, and non-default SMP proof are not claimed by this workspace receipt.

Prerequisite integration update (2026-11-06): the assertion-summary contract
mismatch and storage-scheduling policy error above are resolved in the affected
matrix. The persisted-debugger receipt at
`../add-persisted-debugger-replay/evidence/2026-11-06-persisted-proof.ncl` records
2,703 passing tests, 14 ignored, passing warnings-denied Clippy, and the passing
repaired-Cargo Nix Octet rail (3,879 warnings, zero errors). Exact replay/inspect
now share snapshot-normalized KVM observation boundaries; real single-vCPU
checkpoint and continuation register/memory comparisons agree. The original
workspace receipt remains dated evidence, not rewritten current-source proof.
The prerequisite's non-default SMP Linux execution and before-change baseline
evidence remain open. A fresh workspace sync dry run remains blocked by the
unarchived prerequisite; no sync, archive, dependency removal, or readiness
promotion was executed.
