# Contract/evidence artifact ownership registry.
#
# This is intentionally the first Nickel artifact in the evidence-contract work:
# it records source-of-truth ownership before adding detailed schemas.

let Ownership = std.contract.from_predicate (fun value =>
  value == "nickel-authored"
  || value == "rust-derived"
  || value == "excluded"
) in

let NonEmptyString = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.length value > 0
) in

let RegistryEntry = {
  id | NonEmptyString,
  ownership | Ownership,
  owner | NonEmptyString,
  source_paths | Array NonEmptyString,
  artifact_paths | Array NonEmptyString | default = [],
  validation_commands | Array NonEmptyString,
  fixture_coverage | Array NonEmptyString,
  freshness | NonEmptyString,
  rationale | NonEmptyString,
} in

let Registry = {
  schema_version | NonEmptyString,
  policy | NonEmptyString,
  families | Array RegistryEntry,
} in

{
  schema_version = "1",

  policy = "Nickel owns human-authored run configuration and receipt/review boundaries; Rust owns runtime-emitted records. Raw logs, secrets, cryptographic internals, wire-discriminant ownership, distributed behavior semantics, and hot-path constants are excluded from Nickel ownership.",

  families = [
    {
      id = "run-config",
      ownership = "nickel-authored",
      owner = "contracts/evidence/run-config.ncl",
      source_paths = [
        "contracts/evidence/registry.ncl",
        "contracts/evidence/run-config.ncl",
      ],
      artifact_paths = [
        "dogfood-results/*/run-config.json",
      ],
      validation_commands = [
        "nickel export contracts/evidence/registry.ncl",
        "nickel export contracts/evidence/examples/raft-run-config.ncl",
      ],
      fixture_coverage = [
        "contracts/evidence/fixtures/valid/run-config.valid.json",
        "contracts/evidence/fixtures/invalid/run-config.zero-vms.invalid.json",
        "contracts/evidence/fixtures/invalid/receipt.stale-artifact.invalid.json",
      ],
      freshness = "Nickel-authored source is authoritative; exported JSON is derived and must be regenerated or validated before execution.",
      rationale = "Humans author run inputs and benefit from Nickel defaults, merging, docs, and early validation before long-running VM work starts.",
    },
    {
      id = "dogfood-receipt",
      ownership = "nickel-authored",
      owner = "contracts/evidence/dogfood-receipt.ncl",
      source_paths = [
        "contracts/evidence/registry.ncl",
        "contracts/evidence/dogfood-receipt.ncl",
      ],
      artifact_paths = [
        "dogfood-results/*/receipt.md",
        "dogfood-results/*/receipt.json",
      ],
      validation_commands = [
        "nickel export contracts/evidence/registry.ncl",
        "nickel export contracts/evidence/examples/raft-dogfood-receipt.ncl",
      ],
      fixture_coverage = [
        "contracts/evidence/fixtures/valid/receipt.known-gap.valid.json",
        "contracts/evidence/fixtures/invalid/receipt.missing-hash.invalid.json",
        "contracts/evidence/fixtures/invalid/receipt.missing-replay-attempt.invalid.json",
      ],
      freshness = "Receipt data is the acceptance boundary; generated Markdown must be checked against or produced from validated receipt data.",
      rationale = "Receipts bind commands, git revision, artifacts, hashes, assertion coverage, replay attempts, and known gaps for review.",
    },
    {
      id = "bug-report",
      ownership = "rust-derived",
      owner = "chaoscontrol-explore",
      source_paths = [
        "crates/chaoscontrol-explore/src",
      ],
      artifact_paths = [
        "dogfood-results/*/bug_*.json",
      ],
      validation_commands = [
        "check-evidence-contracts --root .",
        "nickel export contracts/evidence/examples/raft-bug-report.ncl",
      ],
      fixture_coverage = [
        "dogfood-results/raft-20260506-095025/bug_0.json",
        "contracts/evidence/fixtures/invalid/receipt.missing-deterministic-context.invalid.json",
      ],
      freshness = "Rust serde shape is authoritative; Nickel validates the public evidence shape or generated schema freshness.",
      rationale = "Bug reports are emitted runtime facts, not human-authored config, but receipts must prove whether each report is replayable, known-gap, or invalid.",
    },
    {
      id = "assertion-summary",
      ownership = "rust-derived",
      owner = "chaoscontrol-explore",
      source_paths = [
        "crates/chaoscontrol-explore/src",
        "crates/chaoscontrol-sdk/src",
      ],
      artifact_paths = [
        "dogfood-results/*/assertions.json",
        "dogfood-results/*/report.txt",
      ],
      validation_commands = [
        "cargo test -p chaoscontrol-evidence --test assertion_summary_identity",
        "nickel export contracts/evidence/examples/raft-assertion-summary.ncl",
        "check-evidence-contracts --root .",
      ],
      fixture_coverage = [
        "dogfood-results/raft-20260506-095025/assertions.json",
        "contracts/evidence/fixtures/valid/assertions.identity.valid.json",
        "contracts/evidence/fixtures/invalid/assertions.identity-conflict.invalid.json",
        "contracts/evidence/fixtures/invalid/assertions.legacy-promotion.invalid.json",
      ],
      freshness = "Rust assertion identity and report types are authoritative. Nickel reviews the public shape. Rust recomputes fingerprints, catalog tokens, metadata equality, and promotion status.",
      rationale = "Assertion coverage is useful only when each result binds to one admitted descriptor. Legacy summaries remain readable but are never collision-safe promotion evidence.",
    },
    {
      id = "checkpoint-reference",
      ownership = "rust-derived",
      owner = "chaoscontrol-explore",
      source_paths = [
        "crates/chaoscontrol-explore/src/checkpoint.rs",
      ],
      artifact_paths = [
        "dogfood-results/*/checkpoint.json",
        "future:campaign_progress.json",
      ],
      validation_commands = [
        "future:cargo test -p chaoscontrol-explore checkpoint_roundtrip",
        "future:nickel export contracts/evidence/checkpoint-reference-shape.ncl",
      ],
      fixture_coverage = [
        "dogfood-results/raft-20260506-095025/checkpoint.json",
        "future:stale checkpoint reference fixture",
      ],
      freshness = "Full checkpoint schema remains Rust-owned; Nickel receipts validate only references, digests, and replay-relevant metadata.",
      rationale = "Checkpoints can be large runtime state snapshots; receipts should bind them by reference and hash instead of hand-authoring them in Nickel.",
    },
    {
      id = "snapshot-reference",
      ownership = "rust-derived",
      owner = "chaoscontrol-explore snapshot_store",
      source_paths = [
        "crates/chaoscontrol-explore/src/snapshot_store.rs",
        "contracts/evidence/snapshot-ref.ncl",
      ],
      artifact_paths = [
        "dogfood-results/*/snapshots/*.snapshot.bin",
        "dogfood-results/*/bug_*.json",
        "dogfood-results/*/receipt.json",
      ],
      validation_commands = [
        "cargo test -p chaoscontrol-explore snapshot_store",
        "check-evidence-contracts --root .",
      ],
      fixture_coverage = [
        "contracts/evidence/fixtures/valid/snapshot-ref.valid.json",
        "contracts/evidence/fixtures/invalid/snapshot-ref.path-escape.invalid.json",
        "contracts/evidence/fixtures/invalid/snapshot-ref.missing.invalid.json",
        "contracts/evidence/fixtures/invalid/snapshot-ref.corrupt.invalid.json",
        "contracts/evidence/fixtures/invalid/snapshot-ref.wrong-hash.invalid.json",
        "contracts/evidence/fixtures/invalid/snapshot-ref.unsupported-codec.invalid.json",
        "contracts/evidence/fixtures/invalid/snapshot-ref.incompatible-schema.invalid.json",
      ],
      freshness = "Rust snapshot-store code emits the artifact metadata and public refs; Nickel validates the receipt-facing reference shape and safety envelope.",
      rationale = "Replay parent snapshots are runtime-derived evidence artifacts that must be hash-addressed and path-confined without making redb or raw logs a public boundary.",
    },
    {
      id = "replay-verdict",
      ownership = "rust-derived",
      owner = "chaoscontrol-explore replay_verdict",
      source_paths = [
        "crates/chaoscontrol-explore/src/replay_verdict.rs",
        "contracts/evidence/replay-verdict.ncl",
      ],
      artifact_paths = [
        "dogfood-results/*/replay-verdict.json",
        "future:snapshot smoke output replay-verdict.json",
      ],
      validation_commands = [
        "cargo test -p chaoscontrol-explore replay_verdict",
        "check-evidence-contracts --root .",
      ],
      fixture_coverage = [
        "contracts/evidence/fixtures/valid/replay-verdict.snapshot-backed.valid.json",
        "contracts/evidence/fixtures/invalid/replay-verdict.schedule-only-not-proof.invalid.json",
        "contracts/evidence/fixtures/invalid/replay-verdict.missing-snapshot-ref.invalid.json",
        "contracts/evidence/fixtures/invalid/replay-verdict.snapshot-not-reproduced.invalid.json",
      ],
      freshness = "Rust replay verdict code is the semantic source of truth; Nickel/checkers validate public verdict shape and accepted-proof gates.",
      rationale = "Verdicts classify replay attempts without scraping raw logs, separating accepted snapshot-backed proof from schedule-only gaps and invalid evidence.",
    },
    {
      id = "campaign-progress",
      ownership = "rust-derived",
      owner = "chaoscontrol-explore",
      source_paths = [
        "crates/chaoscontrol-explore/src",
        "openspec/specs/campaign-persistence/spec.md",
      ],
      artifact_paths = [
        "future:campaign_progress.json",
        "future:campaign_report.json",
      ],
      validation_commands = [
        "future:cargo test -p chaoscontrol-explore campaign_progress_roundtrip",
        "future:nickel export contracts/evidence/campaign-progress-shape.ncl",
      ],
      fixture_coverage = [
        "future:completed seed progress fixture",
        "future:resume config preservation fixture",
      ],
      freshness = "Rust campaign persistence types are authoritative; Nickel validates review-facing references and summaries.",
      rationale = "Campaign state is runtime progress data, while validated receipts summarize whether it is complete and reviewable.",
    },
    {
      id = "raw-runtime-logs",
      ownership = "excluded",
      owner = "operator/debug tooling",
      source_paths = [
        ".git/info/exclude",
        "dogfood-results/*/run.log",
        "dogfood-results/*/reproduce.log",
      ],
      artifact_paths = [],
      validation_commands = [
        "future:receipt validation may reference raw log paths but must not require committed raw logs",
      ],
      fixture_coverage = [
        "future:receipt with optional raw log reference",
      ],
      freshness = "Raw logs are local debug aids; concise receipts and structured artifacts are durable review evidence.",
      rationale = "Raw logs are voluminous and operational, so they should not become Nickel-owned acceptance records.",
    },
    {
      id = "secrets-and-crypto-internals",
      ownership = "excluded",
      owner = "runtime/security implementation",
      source_paths = [
        "future:any credential, key, token, cryptographic internal, or low-level wire discriminant surface",
      ],
      artifact_paths = [],
      validation_commands = [
        "future:receipt validation rejects secret-like committed fields when concrete receipt schemas exist",
      ],
      fixture_coverage = [
        "future:secret-like receipt field negative fixture",
      ],
      freshness = "Excluded from Nickel ownership; only redacted metadata may appear in receipts when needed.",
      rationale = "Nickel contracts should not own secrets, cryptographic implementation internals, wire discriminants, distributed behavior semantics, or hot-path runtime constants.",
    },
  ],
} | Registry
