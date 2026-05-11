## Phase 1: Spec foundation

- [x] [serial] Create the OpenSpec package for the consistency-checker ecosystem gap.

## Phase 2: History and checker core

- [ ] [serial] Add typed history/event DTOs with deterministic serialization and schema tests.
- [ ] [parallel] Add a checker trait and bounded report model with pass/fail/counterexample outcomes.
- [ ] [parallel] Add known-good and known-bad history fixtures for the first model family.

## Phase 3: Workload integration

- [ ] [depends:history-core] Add a workload adapter path that emits operation histories without raw-log scraping.
- [ ] [depends:checker-core] Add a CLI/report path that runs checkers over committed or freshly generated histories.

## Phase 4: Evidence and gates

- [ ] [depends:integration] Add report/status wording that separates checker evidence from replay proof and assertion-readiness support.
- [ ] [depends:gates] Add negative tests for stale histories, missing outcomes, ambiguous process IDs, and overclaimed support labels.
- [ ] [depends:verification] Verify with pure checker tests, adapter tests, OpenSpec validation, and relevant Nix checks.
