## Context

The current evidence artifacts are useful but not bound tightly enough. A dogfood run can produce `report.txt`, `assertions.json`, `checkpoint.json`, and `bug_0.json`, while the standalone reproducer can still fail because the saved bug report does not carry enough deterministic replay context. The result is honest but manual: reviewers must read the receipt and infer whether the evidence is accepted, partial, or known-bad.

Nickel is a good fit for the human/config and validation boundary, but not for every persisted byte. The design must avoid creating two competing schema owners or requiring large runtime traces to be authored as Nickel.

## Goals / Non-Goals

**Goals:**
- Make run configuration and evidence receipt shape explicit, contracted, and reviewable.
- Validate committed dogfood evidence, including replay success/failure status and known gaps.
- Preserve Rust as the serialization authority for runtime-emitted records while exposing Nickel contracts/checks at artifact boundaries.
- Provide negative fixtures that fail when deterministic replay context, hashes, or required receipt fields are missing.

**Non-Goals:**
- Rewriting checkpoints or raw logs as Nickel.
- Moving secrets, cryptographic internals, low-level wire discriminants, or hot-path runtime constants into Nickel.
- Solving the replay bug itself in this change; this change makes the evidence gap machine-visible and acceptance-blocking.

## Decisions

### 1. Split Nickel-authored configs from Rust-owned records

**Choice:** Human-authored exploration inputs use Nickel modules/contracts and export JSON consumed by Rust. Runtime-emitted records (`bug_*.json`, `checkpoint.json`, assertion summaries, campaign progress) remain Rust-owned Serde artifacts, with Nickel contracts used to validate their public evidence shape or generated from Rust schemas where practical.

**Rationale:** Configs benefit from defaults, merges, docs, and custom validation. Runtime records are facts emitted by the system and should not have two hand-maintained schema sources.

**Rejected alternative:** Make every artifact a hand-authored Nickel record. This would turn checkpoints and high-volume traces into config files and invite schema drift.

### 2. Receipts become the acceptance boundary

**Choice:** A dogfood or campaign receipt records the command, git revision, build outputs, config digest, artifact paths and hashes, assertion coverage, bug reports, replay attempts, acceptance status, and known gaps. Receipt validation must distinguish accepted evidence from partial evidence and known replay failures.

**Rationale:** The Raft run should be reviewable by one validated receipt instead of correlating raw logs, JSON, and prose manually.

**Rejected alternative:** Keep Markdown receipts as the only source of truth. Markdown is useful for review, but it cannot reliably block missing replay context or stale artifact references.

### 3. Raw logs stay referenced, not embedded

**Choice:** Receipts may reference raw logs by ignored local path or optional hash when intentionally preserved, but validation must not require committing voluminous `run.log` or `reproduce.log` files.

**Rationale:** Concise artifacts are better for review. Raw logs are operational debugging aids, not durable acceptance records.

### 4. Validation lands before broad migration

**Choice:** The first implementation slice should add a registry, contracts, fixtures, and checks around the existing Raft dogfood artifacts before expanding to all explorer/campaign outputs.

**Rationale:** This directly targets the observed gap and prevents a large speculative Nickel migration.

## Risks / Trade-offs

- **Schema drift:** Mitigate with generated-contract freshness checks or a typed registry that declares `rust-derived` versus `nickel-authored` ownership per family.
- **False confidence:** Mitigate with negative fixtures for missing replay context, mismatched hashes, stale git revisions, and unvalidated bug reports.
- **Tooling friction:** Mitigate by running Nickel via Nix and keeping Rust output JSON as the interop boundary.
- **Over-contracting:** Mitigate by keeping raw logs, private secrets, and internal VM/hypervisor implementation details out of Nickel.

## Validation Plan

- `nickel typecheck` or `nickel export` succeeds for positive run config and receipt fixtures.
- Negative fixtures fail for missing artifact hashes, missing replay attempts for reported bugs, stale/missing deterministic replay context, and invalid assertion coverage summaries.
- Rust serde tests continue to round-trip runtime records.
- A Nix check validates the committed Raft dogfood receipt and records its current status as a replayability gap, not accepted reproduction evidence.
