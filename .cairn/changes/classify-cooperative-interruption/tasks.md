# Tasks

## Phase 1: Baseline and lifecycle core

- [ ] [serial] Record current signal, explorer, campaign, checkpoint, report, and fault-stage test results before core changes. r[chaoscontrol.exploration_interruption.verification]
- [ ] [serial] Define stop-request, boundary, finalization, interrupted, failed, and forced-exit facts in a narrow pure module. r[chaoscontrol.exploration_interruption.terms] r[chaoscontrol.exploration_interruption.boundary]
- [ ] [serial] Implement deterministic transition, admission, terminal-classification, report-plan, and claim-boundary decisions. r[chaoscontrol.exploration_interruption.request] r[chaoscontrol.exploration_interruption.completion]
- [ ] [parallel] Add positive reducer tests and negative stale, duplicate, premature-terminal, failed-finalization, and terminal-mutation tests. r[chaoscontrol.exploration_interruption.verification]

## Phase 2: Signal and command shells

- [ ] [serial] Make the first signal record request intent only through the established atomic signal boundary. r[chaoscontrol.exploration_interruption.request]
- [ ] [serial] Replace the repeated-signal path with an established async-signal-safe immediate-exit primitive and a named nonzero status. r[chaoscontrol.exploration_interruption.forced_exit]
- [ ] [serial] Route explorer and campaign admission through the lifecycle decision before each new round or seed. r[chaoscontrol.exploration_interruption.completion]
- [ ] [serial] Publish `interrupted` only after the declared boundary and required checkpoint, progress, and report finalization succeed. r[chaoscontrol.exploration_interruption.completion]
- [ ] [serial] Map checkpoint, progress, or report failure to a non-successful interruption class without a cleanup-success claim. r[chaoscontrol.exploration_interruption.completion]

## Phase 3: Evidence and documentation

- [ ] [parallel] Add first-signal subprocess fixtures for explorer and campaign boundary completion, skipped later work, output and no-output policy, and terminal reporting. r[chaoscontrol.exploration_interruption.verification]
- [ ] [parallel] Add repeated-signal subprocess fixtures for immediate nonzero exit and absence of a cooperative terminal marker. r[chaoscontrol.exploration_interruption.forced_exit] r[chaoscontrol.exploration_interruption.verification]
- [ ] [parallel] Add checkpoint-failure, report-failure, stale-phase, duplicate-finalization, and post-request admission rejection fixtures. r[chaoscontrol.exploration_interruption.verification]
- [ ] [parallel] Add scope fixtures that keep harness interruption separate from guest process-kill, restart, and storage-recovery evidence. r[chaoscontrol.exploration_interruption.scope]
- [ ] [serial] Update the glossary, source comments, operator docs, report semantics, and non-claims with the accepted terms. r[chaoscontrol.exploration_interruption.terms] r[chaoscontrol.exploration_interruption.scope]
- [ ] [serial] Run focused tests, workspace tests, Clippy with warnings denied, Octet, Cairn gates, and relevant Nix checks. r[chaoscontrol.exploration_interruption.verification]
