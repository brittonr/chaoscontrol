## 1. Contract foundation

- [x] 1.1 Write the OpenSpec proposal, design, delta spec, and repo guidance for Nickel-backed evidence contracts
- [x] 1.2 Add a typed contract registry that classifies each artifact family as `nickel-authored`, `rust-derived`, or excluded
- [ ] 1.3 Add initial Nickel contracts for run config, dogfood receipt, bug report evidence shape, assertion summary, checkpoint reference, artifact hash, and replay attempt
- [ ] 1.4 Add positive and negative fixtures for the contracts, including the existing Raft dogfood evidence corpus

## 2. Explorer and receipt integration

- [ ] 2.1 Add a Nickel-authored run-config path that exports validated JSON before invoking exploration
- [ ] 2.2 Extend explorer/dogfood receipt writers so every reported bug has a linked replay attempt or an explicit accepted-gap status
- [ ] 2.3 Bind receipts to git revision, command, kernel/initrd store paths, config digest, artifact hashes, assertion coverage, bug files, checkpoint references, and raw-log policy
- [ ] 2.4 Preserve Markdown receipts as human review output generated from or checked against the validated receipt data

## 3. Validation gates

- [ ] 3.1 Add a Nix/local check that runs Nickel validation over positive fixtures and the committed Raft dogfood receipt
- [ ] 3.2 Add negative fixture checks for missing hashes, missing replay attempts, malformed assertion summaries, missing deterministic replay context, and stale artifact references
- [ ] 3.3 Add Rust round-trip/schema tests for any Rust-derived evidence families used by Nickel validation
- [ ] 3.4 Document the acceptance statuses: accepted, partial, known-gap, invalid, and raw-log-only debug evidence

## 4. Dogfood closeout

- [ ] 4.1 Re-validate `dogfood-results/raft-20260506-095025/` through the new contracts and keep its replay failure classified as a known gap
- [ ] 4.2 Run the full local validation bundle for contracts, fixtures, Rust tests, and Nix checks
- [ ] 4.3 Update the final receipt/docs with the validated contract path and remaining replayability follow-up
