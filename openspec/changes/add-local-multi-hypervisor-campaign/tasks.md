## Phase 1: Spec foundation

- [x] [serial] Define the local multi-hypervisor campaign runner scope, non-goals, receipt contract, and validation scenarios.

## Phase 2: Receipt and validation core

- [ ] [serial] Add campaign plan/receipt data model helpers that bind campaign IDs, hypervisor worker IDs, queue entries, leases, receipt paths, summaries, and restart-persistent queue state.
- [ ] [serial] Add fail-closed validators and negative fixtures for duplicate leases/run IDs, missing hypervisor links, missing queue-state evidence, raw-log scraping, and hosted/shared scheduler overclaims.

## Phase 3: Runner integration

- [ ] [depends:receipt-core] Add or extend CLI modes to execute a bounded local multi-hypervisor campaign plan using the existing queue/lease model and thin shell around ChaosControl hypervisor runs.
- [ ] [depends:runner-integration] Capture per-hypervisor run receipts and a campaign summary without raw-log scraping.

## Phase 4: Packaging and readiness

- [ ] [depends:runner-integration] Package the campaign plan, receipt, summary, queue-state proof, and linked per-hypervisor receipts in the replay-readiness Nix output or an explicitly bounded local readiness rail.
- [ ] [depends:packaging] Update generated readiness status/docs to promote only local multi-hypervisor campaign evidence and preserve hosted/multi-machine/product-parity non-claims.
- [ ] [depends:packaging] Verify with focused Rust tests, negative validator tests, OpenSpec validation, readiness report checks, and the smallest relevant Nix readiness build.
