## Tasks

- [x] [serial] Confirm the overlay capture and revert gaps and record the 2026-09-25 depth-three replay failure. r[chaoscontrol.deterministic_snapshots.incremental_lineage]
- [x] [depends:snapshot-lineage-baseline] Add a failing KVM regression that compares a depth-three incremental lineage with a full snapshot. r[chaoscontrol.deterministic_snapshots.incremental_validation]
- [x] [depends:snapshot-lineage-regression] Add pure page-set planning with bounds checks and positive and negative unit tests. r[chaoscontrol.deterministic_snapshots.incremental_core]
- [x] [depends:snapshot-lineage-core] Route incremental capture and restore through the page set and drain the dirty log around host writes. r[chaoscontrol.deterministic_snapshots.incremental_lineage] r[chaoscontrol.deterministic_snapshots.incremental_revert]
- [x] [depends:snapshot-lineage-shell] Compare fresh and long-lived controller branch results from the same parent, including the watchdog replacement path. r[chaoscontrol.deterministic_snapshots.incremental_validation]
- [ ] [depends:snapshot-lineage-shell] Rerun the Raft control and planted-bug campaigns, replay every safety finding at depth three or more, and record overlay sizes per depth. r[chaoscontrol.deterministic_snapshots.incremental_validation]
- [ ] [depends:snapshot-lineage-campaign] Mark pre-change deep findings diagnostic-only in replay and readiness docs, then run workspace tests, KVM smoke tests, and Cairn validation. r[chaoscontrol.deterministic_snapshots.incremental_validation]

## Implementation evidence

`dogfood-results/production-readiness-20261015/attempts.ncl` records the failing-before and passing-after KVM regressions for inherited pages, uncaptured sibling writes, full-overlay restore on a fresh VM, full-image/base handoffs, and public dirty-log observation. Final Pueue task 32 passed workspace all-target tests, Clippy with warnings denied, and formatting. Task 31 passed the real Raft depth-one-through-four full-memory comparisons, persisted-parent continuation in long-lived and fresh controllers, and immediate incremental recapture after full restore. Both 500-tick continuations observed one attributed CPU stall and matched strict execution digests, round observations, and every admitted assertion outcome. Safety invariants held; four bounded sometimes assertions failed identically and 33 catalog sites remained unexercised. A real watchdog replacement event and replay of a deep exploration finding remain unverified, so the corresponding tasks stay open. The enclosing Nickel receipt records the campaign results and release-gate blockers.

The ten-round control and planted campaigns (`cc-production-raft-control-ten-round` and `cc-production-raft-planted-ten-round`) each hit the unchanged 600-second operational bound. Both emitted metrics through round three; their last valid checkpoints retain two rounds and three branches, with eight depth-one sometimes records and no retained safety finding. `campaign-observations.json` retains the exact commands, profiles, checkpoint hashes, records, and limitations. These partial runs do not close the depth-four control or deep-finding replay requirements; no lifecycle sync, archive, or promotion was performed.

## Closure evidence and unchanged task state

`dogfood-results/production-readiness-closure-20261015/attempts.ncl` conservatively continues the seven historical attempts through attempt 20. No further corrective implementation is authorized by that ledger. Unchanged-source release observations are separate from corrective attempts.

The final three-controller proof on clean source `62f97f8244fedcbb83f1d06eaea72c7dbb37e6bd` observed the actual `SI_TIMER` watchdog, poisoned-controller rejection, production replacement, exact retained-parent memory, and immediate incremental recapture. Traces, all admitted assertion outcomes, attributed fault ledgers and rounds matched, but final guest memory differed. `watchdog-current-artifacts-observations.json` preserves the failed verdict, exact inputs and all admitted outcomes once (43 passed, one unexercised of 44 merged sites). The watchdog/continuation task remains unchecked.

`campaign-observations.json` preserves successful early ten-round control/planted runs through depth nine, the later accepted planted depth-eight run with overlay measurements, rejected paired controls, and the unaccepted final kernel cohort. The separate depth-one planted snapshot probe reproduced on the unpatched cohort. No deep safety finding was retained in the listed fresh campaigns; no deep replay success is inferred. The paired campaign/overlay requirement remains unchecked.

The two guest-kernel corrections were not adopted: the first retained an unconditional entropy TSC read; the second stopped during bootstrap with operational `HostWatchdogTimeout`. Their sources and built artifacts remain locally retained, and their activation was backed out within failed attempt 20. Workspace tests, denied-warning Clippy and formatting passed on the retained Rust sources, but the recorded Nix snapshot smoke and native readiness verdicts did not. Fresh clean-source release observations remain a separate gate. No lifecycle sync, archive or promotion is supported by these partial observations.

Final clean-source release observations are now terminal: workspace tests, denied-warning Clippy and formatting passed; full Nix checks failed at `snapshot-replay-smoke`. The local multi-hypervisor report also failed. The required matrix passed six of seven rows and failed `fresh-workload-replay`; its overall verdict is blocked. Strict native validation and policy freshness passed, but traceability still has 120 missing requirements and native readiness failed. The closure receipt binds `62f97f8244fedcbb83f1d06eaea72c7dbb37e6bd`; a later evidence-only commit is not the tested revision.

The selected historical planted probe's exact initrd could not be recovered; its reproduction is not currently repeatable from the retained artifacts. Current upstream Cairn was successfully inspected read-only and still lacks the required downstream self-check applicability integration. These retention and upstream prerequisites are explicit in the closure receipt. No remaining implementation checkbox, lifecycle sync, archive, readiness promotion or production-readiness claim is supported.


## Authorized follow-up evidence

Attempt 21 in `dogfood-results/production-readiness-followup-20261016/attempts.ncl` built the reviewed guest-kernel source with runtime TSC guards and a pre-warmup jitter-clock admission check. `runtime21-observations.json` records `passed=true` for the existing `raft_lineage_proof`: actual `SI_TIMER`/`HostWatchdogTimeout`, poison enforcement, production controller replacement, retained-parent and immediate-recapture fidelity, and equal initial memory, final memory, strict execution identities, every admitted assertion outcome, attributed faults and round observations across long-lived, fresh-full and replacement continuations. Depth-one-through-four overlay payloads were 7,778,304; 10,158,080; 13,348,864; and 14,766,080 bytes. No memory was omitted, declared bound changed, or host timeout classified as a guest bug.

The paired control/planted and selected-replay obligations remain separate and unchecked: attempt-21 control completed ten rounds but rejected its assertion summary, planted completed without a retained safety finding, and the selected probe produced no accepted verdict. Fresh clean-source release gates and native traceability/readiness are still required. This follow-up supersedes the earlier failed-memory conclusion only for its exact recorded artifact cohort; it does not rewrite historical receipts or establish production readiness.

Attempt 23 explicitly selected the existing Linux `clocksource=jiffies notsc` command-line policy without changing public VM defaults or any declared execution bound. `campaign23-observations.json` records accepted collision-safe assertion summaries, ten completed rounds, depth ten and twenty measured overlay captures for each control and `fig8_commit` campaign. Neither retained a safety finding or operational timeout. Both explicit and newly compiled implicit-default selected probes produced `snapshot_backed_reproduced` from valid retained depth-one parents; supported chunk materialization restored their public parent locators. The actual deep-finding replay remains a separate requirement under attempt 24. Native readiness and exact clean-source release gates remain independent.

Attempt 24 completed the separate existing-guest snapshot-probe campaign in 265.820 seconds under the same ten-round, two-branch, 500-tick, seed-42, three-VM and 600-second bounds. It retained six safety findings: five from replay-parent depth three and one from depth four. Every retained deep safety finding reproduced with exact assertion identity, a verified valid retained parent and `snapshot_backed_reproduced`; no operational timeout was reclassified. `deep-probe24-observations.json` retains all six verdicts, exact commands, BLAKE3-bound bug/parent artifacts, catalog outcomes and profile/checkpoint checks. The initial local runner's exit-127 export/replay errors remain retained separately; only those failed command steps were resumed, not the completed campaign. The follow-up profile reuses the existing closure's bitmap/GPA/frontier binding instead of the obsolete blind-mode description. This closes the campaign/deep-replay task only; native readiness, historical artifact recovery and fresh clean-source release gates remain independent.

## Terminal follow-up release boundary

The follow-up `receipt.ncl` and `release-observations.json` bind the exact clean correction candidate `c919a1496963fd7db82b27cf6be6b9e9adc7c65e` separately from clean `9078d39e8870ba141ff41f1f9f98ab6bb2d3bd9d`. The latter passed the strict watchdog proof and all seven KVM matrix rows with `release-eligible`; its native readiness and known failed local campaign report were not promoted.

Attempt 25 passed three report regressions, the actual CLI failed-report rejection and package Clippy with warnings denied. Its full candidate-source gate run is blocked: the initrd, Cargo, formatting, Nix-check and matrix commands timed out during Nix auto-GC/collector-lock waits. No candidate watchdog proof or matrix receipt was emitted, and the diagnostic local app build failed before VM launch. All default workloads and bounds remain required. No synthetic matrix verdict or guest-bug classification replaces those missing results.

The pinned Nickel export passed, verified 388 current manifest entries, preserved historical receipt bytes, and computed `production_ready = false`. Native traceability still records 120 missing and zero dangling references; MCP producer support and the exact historical initrd remain explicit prerequisites. Attempt 25 is blocked, not complete; the authorized ceiling is exhausted. The final release/validation task stays unchecked. No lifecycle sync, archive, readiness promotion or production-readiness claim is supported.

## Authorized terminal extension

`dogfood-results/production-readiness-extension-20261017/` binds the final authorized implementation `6190343c9ffe578f39c13501bb44048c8179e712` to fresh clean-source observations. Workspace all-target tests, denied-warning Clippy, formatting, and the existing three-controller watchdog proof passed. Actual timeout/replacement, full retained-parent memory, incremental recapture, strict execution identities, every admitted assertion outcome, attributed faults, and rounds matched; one of 44 catalog sites remains explicitly unexercised.

The prior completed ten-round control/planted campaigns retain overlays through depth ten, and all six retained deep safety findings reproduced from valid depth-three/four parents. Those results keep their original cohort identities; the current receipt reuses their genuine report/replay evidence rather than rerunning or relabeling them. The documented finding-producing CLI exit 1 is not a campaign failure when validated output and every required replay pass.

All seven current KVM matrix rows ran: five passed; `admitted-drift` and `fresh-workload-replay` timed out at their unchanged 1,800/3,600-second bounds. The overall verdict is `blocked`. Full Nix checks separately timed out at 10,800 seconds. Native validation, policy freshness, and consumer-owned Preserves freshness passed; native readiness still fails with 118 missing requirements, zero dangling references, and a failing MCP smoke check. The exact historical initrd remains unavailable, and no fully passing immutable Cairn producer cohort exists.

The final release/readiness task remains unchecked. The 35-attempt ledger stays blocked; no additional implementation, lifecycle sync/archive, or production-readiness promotion is authorized by these observations. Later evidence/documentation commits are not represented as the tested implementation revision. Maintenance owner: `chaoscontrol-vmm`; producer admission remains with Cairn CLI/core maintainers.

## Prospective continuation through attempt 40

- [x] [depends:snapshot-lineage-shell] Record the user's explicit five-attempt continuation in `dogfood-results/production-readiness-continuation-20261017/attempts.ncl`; retain the original objective, previous ledgers, receipts, declared bounds and ownership restrictions unchanged.
- [ ] [depends:snapshot-lineage-shell] Complete and exercise genuine remaining snapshot/SMP/virtio/runtime acceptance gaps; retain every strict memory, execution, assertion, fault and recovery comparison. r[chaoscontrol.deterministic_snapshots.incremental_validation]
- [ ] [depends:snapshot-lineage-shell] Complete and exercise the actual explorer, workload and evidence consumers needed for native requirement closure; unsupported whole requirements remain open, not annotation debt.
- [ ] [depends:snapshot-lineage-campaign] Diagnose and correct repository-owned Nix/release-wrapper faults, then require fresh Cargo, Nix, native and all-seven-row clean-source report verdicts. Keep unavailable Cairn producer support as an exact owner prerequisite.
- [x] [depends:snapshot-lineage-campaign] Validate a separate BLAKE3-bound Nickel receipt, update only supported task state, commit verified changes and push main without force. Do not claim production readiness unless every original acceptance criterion passes.

This section records prospective authorization and work, not completion evidence. Existing campaign/replay cohorts and historical release receipts are not relabeled as results of the continuation.


## Attempt 36 current-guest consumer correction

- [x] [depends:snapshot-lineage-shell] Preserve the Raft guest's applied application prefix when a node resets volatile protocol state. Exercise restart, catch-up, repeated restart, and changed/truncated-prefix rejection; rebuild the actual guest and rerun the strict three-controller proof without weakening any assertion or declared bound. r[chaoscontrol.deterministic_snapshots.incremental_validation]

The original attempt-36 Nix investigation made no implementation change. The retained counting rule excludes read-only diagnosis, so this is that still-unstarted authorized correction, not attempt 41. Current-source `2aa212f87021391f03717b86089330db0c8967d8` already reproduced `raft-adapter-rollback` in all three compared continuations: the guest retained the log but reset `commit_index` on restart. Missing mandatory virtio execution state remains a decode rejection; no healthy defaults or legacy-snapshot substitution are planned. Frozen-source release commands remain separate and unchanged.


`target/pr40/correction36-application-before/` retains the reproduced empty-prefix failure. The corrected guest passed all 109 library tests and denied-warning Clippy; its Nix-built initrd then passed `target/pr40/correction36-precommit-lineage/runtime-proof.json`. The actual `SI_TIMER` timeout, permanent poison rejection, production replacement, retained-parent memory and immediate incremental recapture passed. All six strict comparison fields matched across long-lived, fresh-full and watchdog-replacement controllers. The 45 admitted sites retain 44 passed and one unexercised outcome, with no safety failure. This pre-commit proof is scoped behavior evidence, not a clean-source release verdict or whole-SMR requirement closure.

## Terminal continuation evidence through attempt 40

`dogfood-results/production-readiness-continuation-20261017/` binds corrected
clean `7dace3fbd283c61e4e8ea0ed59da7e143fbc36fb` separately from prior clean
`2aa212f87021391f03717b86089330db0c8967d8` and later evidence-only delivery
commits. All 40 authorized corrective slots are consumed; previous receipts and
ledgers remain byte-identical. Read-only diagnosis did not create attempt 41.

The actual current-guest proof passed after the applied-prefix correction:
real watchdog interruption, permanent poison enforcement, production controller
replacement, retained-parent and immediate incremental recapture fidelity, and
all six strict comparison fields across three controllers. All admitted
assertion tables match: 44 passed and one unexercised of 45 sites, with no safety
failure. Observed depth-one-through-four overlay payloads are 7,823,360;
11,501,568; 15,151,104; and 17,166,336 bytes. Only the depth-four parent was
persisted. These scoped observations support the existing completed watchdog
task, not universal determinism or whole SMR completion.

The corrected release-profile control/planted/probe explorer commands retain
one accepted round then fail with `Frontier Adapter Campaign BoundExceeded`.
They do not satisfy current ten-round/depth-four completion, and an empty fresh
deep inventory is not a replay. Six original deep findings and the selected
original probe reproduced again with the verified original executable and
original exact inputs. Those results retain their explicit engine cohort;
current readers still reject missing mandatory failure state in the old parents.
No compatibility default or healthy snapshot is substituted.

Fresh workspace all-target tests, denied-warning Clippy and formatting passed.
All seven clean-source KVM rows ran: five passed, `admitted-drift` timed out at
1,800 seconds and `fresh-workload-replay` at 3,600 seconds. The typed overall
matrix verdict is `blocked`. Full Nix checks separately timed out at the
unchanged 10,800-second bound. No timeout is classified as a guest bug.

Frozen-source native traceability remains nonpassing: 266 of 382 requirements
referenced, 116 mandatory missing requirements and zero dangling coverage
references. Supported consumer-owned Preserves freshness passes; MCP smoke does
not. This delivery removes two out-of-scope SMR references from the task note
above, without claiming those whole requirements, and passes actual strict
native validation. The original frozen-source validation failure remains
retained. Every unsupported whole-requirement and final readiness task stays
unchecked.

No passing immutable compatible Cairn producer cohort exists in the admitted
evidence. Required upstream Cargo/Nix/native/Octet gates and the original full
downstream MCP contract remain prerequisites. The exact older missing initrd,
current campaign failure and host-owned Nix dependency wait remain separate
blockers. The new receipt retains exact commands, profiles, artifact identities,
observed faults, assertion coverage, bug files and replay attempts; it never
promotes historical or dirty-source evidence. Maintenance owner remains
`chaoscontrol-vmm`, with producer admission owned by Cairn CLI/core maintainers.

The separate receipt was exported with the recorded Nickel 1.17 cohort;
`receipt-validation.json` records 455 BLAKE3/length-verified retained artifacts,
zero identity mismatches, complete gate/attempt inventories and rejection of
an unsupported `production_ready = true` claim. It computes
`production_ready = false`. Actual strict delivery-metadata validation passed.
Commit `782c534` containing that evidence and the verified source corrections
was fast-forwarded and pushed to `main` without force. This supports only the
delivery checkbox above; the remaining runtime, requirement and release tasks
stay unchecked. Later task-state commits are not release-tested source.

## Authorized corrections 41–45

The user explicitly selected five more attempts after the previous forty slots
were exhausted. The separate prospective ledger is
`dogfood-results/production-readiness-attempts41-45-20261018/attempts.ncl`.
Every original acceptance criterion, report-verdict requirement, declared
execution bound, sibling/host restriction and historical evidence boundary
remains unchanged. This authorization does not supply passing upstream Cairn
or host-owned Nix evidence.

- [x] [depends:snapshot-lineage-shell] Record explicit authorization through attempt45 and validate its Nickel ledger.
- [ ] [depends:snapshot-lineage-shell] Correct the actual frontier failure in attempt41 and exercise the unchanged-bound current control/planted/deep campaigns and actual retained-parent replay.
- [x] [depends:snapshot-lineage-shell] In attempts42–45, implement and exercise genuine remaining owner-local runtime and evidence requirements; keep unsupported whole contracts open.
- [ ] [depends:snapshot-lineage-campaign] Require fresh exact-source Cargo, native, Nix, watchdog and all-seven-row clean-source report verdicts.
- [x] [depends:snapshot-lineage-campaign] Retain a separate BLAKE3-bound Nickel receipt, update only supported task state, and commit/push verified changes without force.

Attempt41 corrected merged per-VM marker capacity and run ownership. The actual
control reached seven rounds and fourteen branches without Frontier
BoundExceeded, but hit the unchanged 600-second operational bound. Its campaign
checkbox remains open. Attempts42/43 exercised complete-VM continuation, pure
preflight, apply-failure poisoning and allocation-free/adversarial virtio
consumers. Attempt44's uninterrupted XMM control failed; attempt45 repaired the
active XSAVE representation without relaxing the required outputs, and added
reserved debug-register admission with valid two-pending-NMI handling.

The integrated precommit workspace, denied-warning Clippy, formatting and
strict real-watchdog Raft proof passed. These are not clean-source release
verdicts. All45 explicitly authorized corrective slots are consumed. Fresh
frozen-source native, Cargo, Nix, campaign/replay, watchdog and seven-row matrix
observations remain required, as do the unresolved Cairn producer and host
prerequisites. No historical receipt is promoted or overwritten.

## Clean-source runtime observations through attempt45

Clean implementation `b51beafc4507cab1fa32bb7a7a3b83d24ed543fc` passed workspace
tests, denied-warning Clippy, formatting and the strict real-watchdog proof.
The separate attempts41–45 receipt owns its exact commands and artifacts;
later evidence-only commits are not this release-tested source.

Control and planted campaigns each retained eight rounds and sixteen branches
before their unchanged 600-second operational timeout. The deliberate deep
probe completed ten rounds and twenty branches; all eight retained safety
findings with recorded parent depth three genuinely reproduced from the
complete validated fresh-directory export. Admitted frontier entry depth
reached three, not four, and the campaign did not emit per-capture overlay
diagnostics. The six historical deep findings and selected original finding
were attempted by the current reader and rejected incomplete CBOR with
`missing field failure`. Original-engine historical successes are not promoted.
The whole campaign/replay task remains unchecked.

The reused `raft_lineage_proof` observed an actual watchdog timeout, poison
enforcement, replacement and equal retained-parent restore/recapture. All
three continuations matched every admitted outcome, memory/execution identity,
fault ledger and round observation; each retained 44 passing, zero failing
and one unexercised catalog site. Its separate depth-one-through-four overlay
payloads were 8241152, 10575872, 14196736 and 16744448 bytes. Those structured
controller-proof measurements do not replace missing campaign measurements.

Native traceability remains 278/382 with 104 mandatory gaps and zero dangling
coverage references. Preserves freshness passes; MCP smoke does not. The five
frozen task-metadata failures remain in that source's gate; later valid
dependency metadata is not a replacement native-readiness pass. No unsupported
requirement, campaign, release or production-readiness task is marked complete.

## Terminal clean-source release observations through attempt45

Every declared release consumer is terminal for clean `b51beaf`. Workspace
output reports 2203 passed, zero failed and 16 ignored tests; denied-warning
Clippy, formatting and strict watchdog continuation passed. Required Nix
checks timed out at the unchanged 10800-second bound. All seven matrix rows
ran: binary/guest builds, deterministic SMP, serialized snapshot replay and
malformed virtio input passed; admitted drift and fresh workload replay timed
out at their unchanged 1800/3600-second bounds. The Rust matrix's overall
verdict is `blocked`, not passing. Empty timed-out row logs do not establish
a guest defect or a particular host/dependency root cause.

The separate Nickel receipt preserves every required gate, all original
acceptance criteria and all earlier receipt bytes. Exact older missing initrd
bytes remain unavailable. A fully passing immutable compatible Cairn producer
cohort, the original full MCP completion contract, remaining ChaosControl
requirement/campaign/compatibility evidence and fresh passing Nix/matrix gates
remain prerequisites. The delivery-only metadata correction is not a frozen
native gate pass. Production readiness remains false; unsupported tasks stay
unchecked and no new corrective slot, sibling edit or host change is implied.

Actual Nickel export and independent BLAKE3/length verification passed for
all 559 retained manifest artifacts, with zero identity mismatches, six
checked helper-source versions and unchanged historical receipt bytes. A
forced `production_ready = true` claim was rejected. The validated receipt
computes a blocked outcome; it does not repair missing historical bytes or
pass native, campaign/replay or release criteria. Strict delivery-only native
metadata validation also passed. Publication remains a separate observable
step; only its completed evidence can support the delivery checkbox above.

Commit `c3be308` containing the verified corrections and separate attempts41–45
evidence was pushed to `main` without force. This supports only the delivery
checkbox above. Campaign/replay, whole-requirement and release-readiness tasks
remain unchecked. This later publication-state note is not part of the clean
`b51beaf` implementation tested by the retained release commands.

## Authorized focused attempt 46

The user's “do it” authorizes one additional cohesive staged-prefix replay
correction and its verification, extending the explicit ceiling from 45 to 46.
`dogfood-results/production-readiness-attempt46-20261018/attempts.ncl` records the
authorization separately; no earlier ledger or receipt is changed. The design's
accepted-prefix advancement retains every publication, seal and durability check.
The campaign checkbox above is pending because the retained clean-source evidence
does not establish the complete depth-four, overlay and historical replay contract.
No implementation or new gate pass is claimed by this authorization note.


## Focused attempt46 runtime evidence

Clean `ad7e892ad6838872432cc227cb1c88bb6f0ae303` advances the accepted staged
control prefix through the existing independent state comparison and seal path.
The cold-replay/stale-control regression failed before the repair and passed
afterward; no admission, publication, reserve or durability check was removed.

The unchanged ten-round, two-branch control and planted profiles completed in
245.729 and 246.746 seconds under 600-second bounds, each with an Accepted 45-site
catalog, 44 passing, zero failing and one unexercised site. The separate deliberate
probe completed in 319.544 seconds and retained eight depth-three safety findings;
every one reproduced with a valid retained parent and `snapshot_backed_reproduced`.
Actual Rust export admitted each checkpoint and matched every retained identity.
All three campaigns retain twenty aligned, per-VM dirty-page overlay measurements.

The admitted traces reach depth three, not required depth four. The campaign and
release checkboxes above therefore remain unchecked; a separate controller depth-four
proof or fresh deliberate probe cannot replace the complete campaign or selected
original historical finding. No extra rounds, wider bounds, validation bypass,
production-readiness promotion, lifecycle sync or archive is supported.

The same clean implementation passed final workspace all-target tests (2204 passed,
zero failed, 16 ignored), denied-warning workspace Clippy and formatting. The
existing strict three-controller proof again observed actual `SI_TIMER` timeout,
poison enforcement, production replacement, retained-parent and recapture fidelity,
and equality of initial/final memory, strict execution identities, every admitted
assertion outcome, fault ledgers and rounds. Its depth-one-through-four overlays
are separately identified controller-proof evidence, not campaign depth four.

The separate focused receipt computes production readiness from every unchanged
criterion. Old native/Nix/matrix failures and incomplete historical artifacts are
not current-source passes. Historical receipts remain byte-identical. No whole
campaign/release checkbox, lifecycle sync or archive is closed by this delivery.

## Authorized focused attempt47

The explicit "Authorize attempt 47" selection permits one owner-local frontier
proposal/diversity correction and its verification, extending the ceiling to47.
The separate `production-readiness-attempt47-20261018/attempts.ncl` ledger preserves
the original criteria and every earlier receipt. Logical-marker representatives
must retain all observed events, exact selected context and existing shared
rank/admission/publication semantics. This note is authorization, not a gate pass.

- [x] [depends:snapshot-lineage-shell] Record focused attempt47 authority and the read-only depth-retention diagnosis before implementation.
- [x] [depends:snapshot-lineage-shell] Correct logical-marker proposal multiplicity with consumer regression evidence and unchanged Campaign parity.
- [x] [depends:snapshot-lineage-campaign] Complete unchanged-bound depth-four campaigns, record overlays and replay every retained deep safety finding.
- [ ] [depends:snapshot-lineage-campaign] Retain exact-source Cargo/watchdog and Nickel/BLAKE3 evidence; preserve independent readiness blockers and publish verified changes without force.

The bounded-frontier regression failed before the correction: Campaign selected
depth1 instead of the distinct depth2 child. After normalization, both new
regressions and the complete explorer library passed (418 passed, zero failed,
one ignored), including the existing Campaign/legacy parity corpus. Distinct
logical keys and owners retain their own exact context; every observed hit still
contributes to rarity decay.

Clean `b5f0a4550f86cfa3f5b9f45206a57d4087225182` completed the unchanged
ten-round, twenty-branch control, planted and deliberate-probe profiles in
280.479, 283.577 and 307.813 seconds under 600-second bounds. Maximum admitted
depths are 8, 8 and 4; every campaign has twenty aligned overlay captures with
depth-one-through-four payload measurements. All eight retained depth-three
safety findings reproduced with valid retained parents and
`snapshot_backed_reproduced` verdicts. The selected original planted finding
still lacks a current-source snapshot-backed replay because its exact historical
initrd is unavailable.

Fresh exact-source workspace tests passed 2206 with zero failures and 16
ignored; denied-warning Clippy and formatting passed. Native traceability still
reports 103 missing requirements, zero dangling references and verdict `fail`.
Three fresh strict watchdog attempts exited 1 with
`HostWatchdogTimeout { vcpu: 0 }` under host load of 34–44 on 32 CPUs; these are
operational host timeouts, not guest findings. The attempt46 proof passed on
`ad7e892` and remains separate prior evidence, not a fresh pass. Nix and the
seven-row KVM matrix were not rerun. All 47 authorized corrective attempts are
consumed; the remaining criteria stay open.


## Unchanged-source release verification, 2026-10-20

Clean `29329c854338702457eb9967c4830b79487a8901` passes all seven required KVM
release-matrix rows with terminal class `release-eligible`. The independent
matrix checker passes, and all 138 retained row artifacts match their recorded
BLAKE3 identities and lengths. The fresh Raft seed-zero probe is accepted with
`snapshot_backed_reproduced`, valid parent depth one and successful export/replay.
It does not replace the selected original historical planted finding.

The full Nix check fails at `dependency-policy`: the negative controls mutate
obsolete HTTPS source allowlist entries after the Radicle repin. No new product
correction is made or authorized by these gate-only runs. Cairn MCP smoke and
the exact historical initrd remain prerequisites. The separate append-only
`dogfood-results/production-readiness-gates-20261020/receipt.ncl` records the
commands, clean source, verdicts and boundaries; historical receipts stay
unchanged. No overall release, lifecycle sync or archive task is closed.

The earlier TSC-only diagnosis and claimed 240-second timeout overrun are not
established: one jiffies success does not prove sole causality, and 3:44 elapsed
is only 224 seconds. Host load alone is not proof of a timeout's cause.
