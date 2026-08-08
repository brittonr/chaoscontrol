## Phase 1: Spec foundation

- [x] [serial] Define networked hosted scheduler harness scope, requirements, design, and verification plan.

## Phase 2: Receipt and validator model

- [x] [depends:spec-foundation] Add networked hosted scheduler plan/receipt DTOs for worker sessions, heartbeats, leases, queue revisions, state snapshot digests, decision revisions, and linked run receipts.
- [x] [depends:receipt-model] Add positive and negative validators for duplicate leases, stale queue/decision revisions, missing worker sessions, missing passed-run summaries, raw-log scraping, and hosted/product parity overclaims.

## Phase 3: Harness and CLI

- [x] [depends:validator] Implement a bounded local networked or multi-process harness that starts at least two worker identities against a shared queue/decision-store adapter.
- [x] [depends:harness] Extend the scheduler receipt CLI with sample/run/check modes for the networked hosted scheduler harness.

## Phase 4: Packaging and readiness

- [x] [depends:cli] Package the plan, receipt, worker-session records, queue-state snapshots, decision-store snapshots, and linked run receipts in the replay-readiness Nix check.
- [x] [depends:packaging] Regenerate replay readiness docs and promotion-gate selftests so loopback evidence remains bounded and networked evidence is required for stronger hosted/fleet claims.
- [x] [depends:verification] Verify with focused model tests, CLI smoke, generated report check, strict OpenSpec validation, `git diff --check`, and `nix build .#checks.x86_64-linux.replay-readiness --no-link -L`.
