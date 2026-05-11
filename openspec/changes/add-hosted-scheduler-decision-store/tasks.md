## Phase 1: Spec foundation

- [x] [serial] Define hosted scheduler/shared decision-store scope, non-goals, receipt contract, and validation scenarios.

## Phase 2: Shared-state contracts

- [ ] [serial] Add shared queue and decision-store data models that bind machine IDs, hypervisor worker IDs, queue entries, lease IDs/epochs, run receipt paths, replay-readiness summaries, decision revisions, and writer identities.
- [ ] [serial] Add fail-closed validators and negative fixtures for duplicate lease ownership, stale lease epochs, missing machine/run links, stale decision writes, split-brain decision records, raw-log scraping, and hosted/product-parity overclaims.

## Phase 3: Bounded hosted/fleet harness

- [ ] [depends:shared-state-contracts] Add a bounded local two-machine or loopback harness that exercises shared queue leasing and shared decision writes through the adapter boundary.
- [ ] [depends:bounded-harness] Capture per-machine run receipts, shared queue-state snapshots, shared decision records, and a hosted/fleet summary without raw-log scraping.

## Phase 4: Packaging and readiness

- [ ] [depends:bounded-harness] Package the shared queue plan, hosted/fleet receipt, shared decision-store receipt, state snapshots, and linked per-run replay-readiness receipts in a bounded readiness rail.
- [ ] [depends:packaging] Update generated readiness status/docs only to the supported bounded shared-state level proven by receipts, preserving non-claims for SaaS hosting, universal fleet scale, and Antithesis parity.
- [ ] [depends:packaging] Verify with focused Rust tests, negative validator tests, OpenSpec validation, readiness report checks, and the smallest relevant Nix readiness build.
