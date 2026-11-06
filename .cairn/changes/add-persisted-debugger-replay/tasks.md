# Tasks

## Phase 1: Reviewed planning foundation

- [x] [serial] Inspect recording/checkpoint serde, debugger and exact replay continuation, real-runner configuration intake, explorer snapshot persistence and reproduction, and overlapping lineage/storage/witness plans; record immutable RewindVM reference and ownership boundaries. r[chaoscontrol.replay_debugger.session] r[chaoscontrol.replay_debugger.shared_store] r[chaoscontrol.replay_debugger.navigation]
- [x] [serial] Author the proposal, design, delta requirements, and implementation/evidence plan; pass the package's proposal and design gates with the Cairn revision pinned by the repository policy. This completes planning only. r[chaoscontrol.replay_debugger.admission] r[chaoscontrol.replay_debugger.producer] r[chaoscontrol.replay_debugger.failure_isolation] r[chaoscontrol.replay_debugger.claim_boundary] r[chaoscontrol.replay_debugger.validation]

## Phase 2: Baseline and shared persisted-session intake

- [x] [serial] Before source changes, run focused replay/explorer/evidence baselines and capture a fresh-process failure of the current metadata-only recording path; discover exported store/runner/recording references before changing APIs and list every caller to migrate. r[chaoscontrol.replay_debugger.shared_store] r[chaoscontrol.replay_debugger.validation]
- [x] [serial] Move the existing bounded snapshot store/codec into the replay library, migrate explorer imports and consumers atomically, preserve existing serialized snapshot references, and remove the obsolete module without duplicate readers or compatibility re-exports. r[chaoscontrol.replay_debugger.shared_store]
- [x] [serial] Implement the Rust-owned versioned exact-session envelope, explicit ordered checkpoint inventory, complete deterministic configuration and artifact/run/assertion bindings, domain-separated BLAKE3 session identity, and Nickel-backed named finite limits. Do not infer omitted exact inputs. r[chaoscontrol.replay_debugger.session] r[chaoscontrol.replay_debugger.claim_boundary]
- [x] [serial] Implement pure complete-session/selected-restore admission and bounded capability-rooted no-follow artifact loading, including decoded snapshot, topology, prefix, digest, target-range, arithmetic, and host-capability checks before machine effects; reject requested-checkpoint boot fallback. r[chaoscontrol.replay_debugger.admission] r[chaoscontrol.replay_debugger.failure_isolation]

## Phase 3: Producer and exact navigation cutover

- [x] [serial] Add `reproduce --debug-recording-output` to the existing explorer flow, retain original finding provenance, capture a post-counterfactual-setup start checkpoint and exact completed-round traces, and publish a complete manifest only after all retained references validate without changing reproduction verdict/exit semantics. r[chaoscontrol.replay_debugger.producer] r[chaoscontrol.replay_debugger.session]
- [x] [serial] Unify replay and debugger continuation validation, construct real runners from the admitted complete execution descriptor, wire existing replay/inspect CLI intake to persisted sessions, and enforce deterministic checkpoint choice and exact target arrival. Keep old metadata input diagnostic-only. r[chaoscontrol.replay_debugger.navigation] r[chaoscontrol.replay_debugger.admission] r[chaoscontrol.replay_debugger.claim_boundary]
- [x] [serial] Isolate failed/poisoned runners and unverified live state, require fresh admission/runner creation after a failed attempt, and preserve explicit counterfactual labeling for existing memory/register/schedule mutation without redefining mutation persistence. r[chaoscontrol.replay_debugger.failure_isolation] r[chaoscontrol.replay_debugger.claim_boundary]

## Phase 4: Consumer-visible fixtures and actual CLI proof

- [x] [parallel] Add paired portable behavioral fixtures for fresh-session reload, non-default exact configuration, artifact relocation, exact start/end targets, empty evidence slices, and replay/debugger fault-and-SMP validation parity; include missing/corrupt/incompatible state, unsafe paths, identity/prefix/trace drift, overflow/bounds, early stop, output failure, and poison reuse negatives. Do not test source text or forwarding alone. r[chaoscontrol.replay_debugger.session] r[chaoscontrol.replay_debugger.admission] r[chaoscontrol.replay_debugger.navigation] r[chaoscontrol.replay_debugger.failure_isolation] r[chaoscontrol.replay_debugger.validation]
- [ ] [serial] Through pueue, exercise an admitted Rust finding under a bounded non-default SMP profile: produce a session, exit the producer, reload in separate replay and inspect CLI processes, seek backward/forward across checkpoints, and compare declared memory/register values and canonical continuation with the control. Tamper a trace and remove a snapshot in isolated copies and observe explicit rejection. Keep affected deep lineage or unavailable KVM blocked. r[chaoscontrol.replay_debugger.producer] r[chaoscontrol.replay_debugger.navigation] r[chaoscontrol.replay_debugger.validation]
- [x] [serial] Record a validated bounded receipt with exact source, commands, profile/seed/artifact identities, inspected observations, positive/negative results, retention locations, and non-claims. Update existing replay/retention documentation after smoke proof; do not promote a debugger seek into an accepted replay verdict. r[chaoscontrol.replay_debugger.claim_boundary] r[chaoscontrol.replay_debugger.validation]
- [x] [serial] After implementation, run focused replay/explorer/evidence tests and Clippy, applicable evidence/Nix KVM rails, Cairn validation and required gates. Leave implementation checkboxes open until those observations exist; do not sync/archive from a spec-only delivery. r[chaoscontrol.replay_debugger.validation]

Evidence: `evidence/2026-11-06-persisted-proof.ncl` exports through pinned Nickel.
The final affected matrix passes 2,703 tests across 168 targets (14 ignored),
warnings-denied Clippy passes, and the repaired-Cargo Nix Octet rail reports
warning-only with zero errors. Actual KVM reproduction publishes a finding-bound
session; separate replay/inspect processes navigate ticks 1835/1836 and agree on
all inspected registers and selected memory after pending-I/O settlement. Both
CLI paths reject tampered traces and missing snapshots. Bootstrap capture retains
checkpoints at 0/1175/1180. Native validation and proposal/design/tasks gates pass;
these structural results do not discharge the unchecked execution prerequisite.

Remaining execution prerequisite: the required non-default SMP Linux run remains
open until its IOAPIC timer boot failure is repaired and the full proof executes.
Baseline recovery: `evidence/2026-11-06-baseline-recovery.ncl` retains the original
2,726-pass/six-failure matrix before cutover work and the cached pre-cutover CLI's
fresh-process failure, plus original exported-reference discovery and caller list.
The cached-program probe preceded rebuilding but overlapped a source-file move;
that sequencing deviation is retained, not rewritten as strictly serial editing.
The checked baseline row records original-version behavioral evidence, not a new
run or a claim that the historical timing deviation was undone. No sync/archive
has yet executed; the dependent workspace still awaits this prerequisite.
