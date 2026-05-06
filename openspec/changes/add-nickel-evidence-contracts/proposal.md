## Why

The Raft dogfood run produced useful evidence, but it also exposed a receipt-integrity gap: the explorer persisted a reported bug schedule while standalone reproduction did not replay the assertion failure from the saved `bug_0.json`. ChaosControl currently treats run configuration, bug records, assertion summaries, and receipts as loosely related artifacts. That makes it too easy for a run to look accepted even when the saved evidence is missing deterministic replay context.

Nickel contracts give the project a reviewable boundary for human-authored run configuration and machine-emitted evidence without turning runtime logs into configuration. This change specifies which surfaces Nickel owns, which Rust still owns, and how validation becomes part of dogfood acceptance.

## What Changes

- Add a Nickel-backed evidence contract capability for run configs, dogfood receipts, bug reports, assertion summaries, and checkpoint references.
- Define source-of-truth boundaries: Nickel-authored modular configs for human inputs; Rust-derived or Rust-owned schemas for machine-emitted records; raw logs remain out of contract scope.
- Require validated receipts to bind command, git revision, built artifacts, run config, hashes, assertion coverage, bug reports, replay attempts, and known gaps.
- Add acceptance gates that validate positive and negative fixtures before dogfood evidence can be treated as review-ready.

## Capabilities

### New Capabilities
- `nickel-evidence-contracts`: Contracted configuration and evidence receipts for exploration/dogfood runs.

### Modified Capabilities
- `campaign-runner`: Run and campaign outputs gain validated evidence receipt requirements.
- `campaign-persistence`: Persisted progress and bug/checkpoint references become part of receipt validation.

## Impact

- **Files**: likely `contracts/`, `dogfood-results/**/receipt.*`, explorer receipt writers, fixture directories, and Nix checks.
- **Dependencies**: Nickel CLI available through Nix for validation/export; Rust remains the authority for runtime record serialization.
- **Testing**: Nickel typecheck/export checks, positive/negative fixture validation, Rust serde round-trip tests, and dogfood receipt validation against the Raft evidence corpus.
- **Non-goals**: no container image intake, no language-agnostic SDK expansion, no hand-authoring of high-volume checkpoints, and no secret/credential material in receipts.
