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

  policy = "Nickel owns bounded human-authored run, simulator, campaign, finite-schedule, typed operator-command, KVM release-matrix, and receipt-review inputs. Rust owns config construction, external-input revalidation, progress, traces, checkpoints, outcomes, reports, receipts, execution, and replay. Raw logs, secrets, cryptographic internals, wire-discriminant ownership, distributed behavior semantics, and hot-path constants are excluded from Nickel ownership.",

  families = [
    {
      id = "run-config",
      ownership = "nickel-authored",
      owner = "contracts/evidence/run-config.ncl",
      source_paths = [
        "contracts/evidence/profile-primitives.ncl",
        "contracts/evidence/run-config.ncl",
        "contracts/evidence/examples/raft-run-config.ncl",
      ],
      artifact_paths = [
        "contracts/evidence/fixtures/valid/run-profile.valid.json",
        "contracts/evidence/fixtures/valid/run-profile.projection-receipt.json",
      ],
      validation_commands = [
        "check-profile-projections --root .",
        "nickel export contracts/evidence/examples/raft-run-config.ncl",
        "check-profile-admission run contracts/evidence/fixtures/valid/run-profile.valid.json contracts/evidence/fixtures/valid/run-profile.projection-receipt.json",
        "cargo test -p chaoscontrol-explore profile::tests",
      ],
      fixture_coverage = [
        "contracts/evidence/fixtures/valid/run-profile.valid.json",
        "contracts/evidence/fixtures/invalid/run-profile.unknown-mode.invalid.ncl",
        "contracts/evidence/fixtures/invalid/run-profile.unknown-field.invalid.ncl",
        "contracts/evidence/fixtures/invalid/run-profile.zero-budget.invalid.ncl",
        "contracts/evidence/fixtures/invalid/run-profile.unsafe-reference.invalid.ncl",
        "contracts/evidence/fixtures/invalid/run-profile.implicit-blind-coverage.invalid.ncl",
      ],
      freshness = "The checked projection binds the Nickel source, shared contract, evaluator identity, profile identity, and JSON with BLAKE3 before runtime admission.",
      rationale = "Humans author bounded VM exploration intent; Rust revalidates the projection and owns construction and execution.",
    },
    {
      id = "typed-operator-command",
      ownership = "nickel-authored",
      owner = "contracts/evidence/operator-command.ncl",
      source_paths = [
        "contracts/evidence/profile-primitives.ncl",
        "contracts/evidence/operator-command.ncl",
        "contracts/evidence/examples/typed-operator-command.ncl",
      ],
      artifact_paths = [
        "contracts/evidence/fixtures/valid/operator-command.valid.json",
      ],
      validation_commands = [
        "check-evidence-contracts --root .",
        "nickel export contracts/evidence/examples/typed-operator-command.ncl",
        "cargo test -p chaoscontrol-evidence typed_operator_command",
      ],
      fixture_coverage = [
        "contracts/evidence/fixtures/valid/operator-command.valid.json",
        "contracts/evidence/fixtures/invalid/operator-command.free-form.invalid.ncl",
        "contracts/evidence/fixtures/invalid/operator-command.traversal.invalid.ncl",
      ],
      freshness = "The checked Nickel projection must match the committed JSON and deserialize into the Rust DTO before process creation.",
      rationale = "Nickel owns reviewed command intent. Rust revalidates identity and policy, then bounded-exec owns process mechanics.",
    },
    {
      id = "simulator-profile",
      ownership = "nickel-authored",
      owner = "contracts/evidence/simulator-profile.ncl",
      source_paths = [
        "contracts/evidence/profile-primitives.ncl",
        "contracts/evidence/simulator-profile.ncl",
        "contracts/evidence/examples/register-simulator-profile.ncl",
      ],
      artifact_paths = [
        "contracts/evidence/fixtures/valid/simulator-profile.valid.json",
        "contracts/evidence/fixtures/valid/simulator-profile.projection-receipt.json",
      ],
      validation_commands = [
        "check-profile-projections --root .",
        "check-profile-admission simulator contracts/evidence/fixtures/valid/simulator-profile.valid.json contracts/evidence/fixtures/valid/simulator-profile.projection-receipt.json",
        "cargo test -p chaoscontrol-evidence simulator_profile",
      ],
      fixture_coverage = [
        "contracts/evidence/fixtures/valid/simulator-profile.valid.json",
        "contracts/evidence/fixtures/invalid/simulator-profile.unsupported-rng.invalid.ncl",
        "contracts/evidence/fixtures/invalid/simulator-profile.empty-artifacts.invalid.ncl",
        "contracts/evidence/fixtures/invalid/simulator-profile.malformed-digest.invalid.ncl",
        "contracts/evidence/fixtures/invalid/simulator-profile.weakened-scope.invalid.ncl",
      ],
      freshness = "Generation binds source, imports, evaluator, profile, and projection identities before SimulatorConfig construction.",
      rationale = "Nickel validates simulator intent while Rust retains config construction, execution, observations, and receipts.",
    },
    {
      id = "campaign-profile",
      ownership = "nickel-authored",
      owner = "contracts/evidence/campaign-profile.ncl",
      source_paths = [
        "contracts/evidence/profile-primitives.ncl",
        "contracts/evidence/run-config.ncl",
        "contracts/evidence/campaign-profile.ncl",
        "contracts/evidence/examples/raft-campaign-profile.ncl",
      ],
      artifact_paths = [
        "contracts/evidence/fixtures/valid/campaign-profile.valid.json",
        "contracts/evidence/fixtures/valid/campaign-profile.projection-receipt.json",
      ],
      validation_commands = [
        "check-profile-projections --root .",
        "check-profile-admission campaign contracts/evidence/fixtures/valid/campaign-profile.valid.json contracts/evidence/fixtures/valid/campaign-profile.projection-receipt.json",
        "cargo test -p chaoscontrol-explore profile::tests",
      ],
      fixture_coverage = [
        "contracts/evidence/fixtures/valid/campaign-profile.valid.json",
        "contracts/evidence/fixtures/invalid/campaign-profile.duplicate-seeds.invalid.ncl",
        "contracts/evidence/fixtures/invalid/campaign-profile.incompatible-topology.invalid.ncl",
        "contracts/evidence/fixtures/invalid/campaign-profile.inverted-havoc.invalid.ncl",
        "contracts/evidence/fixtures/invalid/campaign-profile.invalid-workers.invalid.ncl",
        "contracts/evidence/fixtures/invalid/campaign-profile.implicit-metrics.invalid.ncl",
      ],
      freshness = "The campaign projection and receipt are checked before threads or VMs start; progress and reports never flow back into this authored profile.",
      rationale = "Reviewed campaign intent is mergeable while completed seeds, failures, coverage, and reports remain runtime facts.",
    },
    {
      id = "finite-fault-schedule-profile",
      ownership = "nickel-authored",
      owner = "contracts/evidence/fault-schedule-profile.ncl",
      source_paths = [
        "contracts/evidence/profile-primitives.ncl",
        "contracts/evidence/fault-schedule-profile.ncl",
        "contracts/evidence/examples/raft-fault-schedule-profile.ncl",
      ],
      artifact_paths = [
        "contracts/evidence/fixtures/valid/fault-schedule-profile.valid.json",
        "contracts/evidence/fixtures/valid/fault-schedule-profile.projection-receipt.json",
      ],
      validation_commands = [
        "check-profile-projections --root .",
        "check-profile-admission schedule contracts/evidence/fixtures/valid/fault-schedule-profile.valid.json contracts/evidence/fixtures/valid/fault-schedule-profile.projection-receipt.json",
        "cargo test -p chaoscontrol-explore profile::tests",
      ],
      fixture_coverage = [
        "contracts/evidence/fixtures/valid/fault-schedule-profile.valid.json",
        "contracts/evidence/fixtures/invalid/fault-schedule.out-of-range-target.invalid.ncl",
        "contracts/evidence/fixtures/invalid/fault-schedule.unordered.invalid.ncl",
        "contracts/evidence/fixtures/invalid/fault-schedule.overlapping-partition.invalid.ncl",
        "contracts/evidence/fixtures/invalid/fault-schedule.unknown-action.invalid.ncl",
      ],
      freshness = "The descriptor projection is source-bound before Rust converts it into a FaultSchedule.",
      rationale = "Nickel catches malformed finite intent; Rust owns applicability, application, observation, and outcomes.",
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
        "contracts/evidence/assertion-evidence-identity.ncl",
        "contracts/evidence/bug-report.ncl",
      ],
      artifact_paths = [
        "dogfood-results/*/bug_*.json",
      ],
      validation_commands = [
        "check-evidence-contracts --root .",
        "nickel export contracts/evidence/examples/raft-bug-report.ncl",
        "nickel export contracts/evidence/examples/bug-report-identity.ncl",
      ],
      fixture_coverage = [
        "dogfood-results/raft-20260506-095025/bug_0.json",
        "contracts/evidence/fixtures/valid/bug-report.identity.valid.json",
        "contracts/evidence/fixtures/invalid/bug-report.alias-substitution.invalid.ncl",
        "contracts/evidence/fixtures/invalid/bug-report.legacy-descriptor.invalid.ncl",
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
        "contracts/evidence/fixtures/invalid/replay-verdict.schema-v1-accepted.invalid.json",
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
        ".cairn/specs/campaign-persistence/spec.md",
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
      id = "simulator-runtime-records",
      ownership = "rust-derived",
      owner = "chaoscontrol-evidence in_process_simulator",
      source_paths = [
        "crates/chaoscontrol-evidence/src/in_process_simulator.rs",
      ],
      artifact_paths = [
        "future:simulator observations, summaries, and receipts",
      ],
      validation_commands = [
        "cargo test -p chaoscontrol-evidence in_process_simulator",
      ],
      fixture_coverage = [
        "crates/chaoscontrol-evidence/src/in_process_simulator.rs::tests",
      ],
      freshness = "Rust types and validators own observed simulator facts; profiles cannot populate these records.",
      rationale = "Observed steps, entropy, faults, outputs, comparisons, and receipts are runtime facts.",
    },
    {
      id = "execution-traces",
      ownership = "rust-derived",
      owner = "chaoscontrol-trace and chaoscontrol-replay",
      source_paths = [
        "crates/chaoscontrol-trace/src",
        "crates/chaoscontrol-replay/src",
      ],
      artifact_paths = [
        "future:bounded trace and replay artifacts",
      ],
      validation_commands = [
        "cargo test -p chaoscontrol-trace -p chaoscontrol-replay",
      ],
      fixture_coverage = [
        "crates/chaoscontrol-trace/src",
        "crates/chaoscontrol-replay/src",
      ],
      freshness = "Trace schemas remain Rust-owned and are not imported into authored profile values.",
      rationale = "A profile can reference a trace policy but cannot author an observed execution trace.",
    },
    {
      id = "fault-outcomes",
      ownership = "rust-derived",
      owner = "chaoscontrol-fault and chaoscontrol-vmm",
      source_paths = [
        "crates/chaoscontrol-fault/src/engine.rs",
        "crates/chaoscontrol-vmm/src/controller.rs",
      ],
      artifact_paths = [
        "future:fault attempt and outcome evidence",
      ],
      validation_commands = [
        "cargo test -p chaoscontrol-fault -p chaoscontrol-vmm",
      ],
      fixture_coverage = [
        "crates/chaoscontrol-fault/tests",
      ],
      freshness = "Runtime attempt, application, observation, and effect classification remain Rust-derived.",
      rationale = "A finite schedule descriptor is intent and cannot self-promote into a fault outcome.",
    },
    {
      id = "campaign-reports",
      ownership = "rust-derived",
      owner = "chaoscontrol-explore campaign",
      source_paths = [
        "crates/chaoscontrol-explore/src/campaign.rs",
        "crates/chaoscontrol-explore/src/checkpoint.rs",
      ],
      artifact_paths = [
        "future:campaign_report.json",
        "future:campaign_report.txt",
        "future:campaign_progress.json",
      ],
      validation_commands = [
        "cargo test -p chaoscontrol-explore campaign",
      ],
      fixture_coverage = [
        "crates/chaoscontrol-explore/src/campaign.rs::tests",
      ],
      freshness = "Completed seeds, failures, bugs, checkpoints, and reports are validated after execution and never merged into authored profiles.",
      rationale = "Campaign reports describe observations and completion state, not reviewable pre-run intent.",
    },
    {
      id = "kvm-release-matrix",
      ownership = "nickel-authored",
      owner = "contracts/kvm-release/profile.ncl",
      source_paths = [
        "contracts/kvm-release/profile.ncl",
        "contracts/kvm-release/matrix.ncl",
        "crates/chaoscontrol-evidence/src/kvm_release",
        "crates/chaoscontrol-evidence/src/bin/run-kvm-release-matrix.rs",
      ],
      artifact_paths = [
        "contracts/kvm-release/matrix.json",
        "target/kvm-release/release-receipt.json",
        "target/kvm-release/release-summary.md",
        "dogfood-results/kvm-release-evidence-20260809/validation-receipt.json",
      ],
      validation_commands = [
        "check-kvm-release-matrix --root .",
        "run-kvm-release-matrix --root . --matrix contracts/kvm-release/matrix.json --dry-run",
        "cargo test -p chaoscontrol-evidence kvm_release",
      ],
      fixture_coverage = [
        "contracts/kvm-release/fixtures/valid/complete-receipt.valid.json",
        "contracts/kvm-release/fixtures/invalid/missing-timeout.invalid.ncl",
        "contracts/kvm-release/fixtures/invalid/missing-row.invalid.json",
        "contracts/kvm-release/fixtures/invalid/stale.invalid.json",
        "contracts/kvm-release/fixtures/invalid/skipped.invalid.json",
        "contracts/kvm-release/fixtures/invalid/unsupported.invalid.json",
        "contracts/kvm-release/fixtures/invalid/timed-out.invalid.json",
        "contracts/kvm-release/fixtures/invalid/tampered.invalid.json",
        "contracts/kvm-release/fixtures/invalid/dirty.invalid.json",
        "contracts/kvm-release/fixtures/invalid/overclaim.invalid.json",
      ],
      freshness = "The worker binds the selected source revision, matrix identity, admitted host facts, exact commands, finite limits, row outcomes, and retained BLAKE3 artifact identities in one time-bounded receipt.",
      rationale = "Humans review the required KVM release cohort in Nickel while Rust observes the worker and classifies runtime evidence without promoting skipped or unsupported rows.",
    },
    {
      id = "state-machine-property-profiles",
      ownership = "nickel-authored",
      owner = "contracts/property-coverage/profile.ncl",
      source_paths = [
        "contracts/property-coverage/profile.ncl",
        "contracts/property-coverage/profiles.ncl",
        "crates/chaoscontrol-property-suite/src",
      ],
      artifact_paths = [
        "contracts/property-coverage/profiles.json",
        "target/property-coverage/fast-receipt.json",
        "target/property-coverage/deep-receipt.json",
        "dogfood-results/state-machine-property-coverage-20260809/fast-receipt.json",
        "dogfood-results/state-machine-property-coverage-20260809/deep-receipt.json",
      ],
      validation_commands = [
        "nix build .#checks.x86_64-linux.property-coverage -L",
        "cargo test -p chaoscontrol-property-suite",
        "cargo run -p chaoscontrol-property-suite --bin run-property-lane -- --lane deep --output target/property-coverage/deep-receipt.json",
      ],
      fixture_coverage = [
        "contracts/property-coverage/fixtures/invalid/zero-steps.invalid.ncl",
        "contracts/property-coverage/fixtures/invalid/overclaim.invalid.ncl",
        "contracts/property-coverage/fixtures/regressions/scheduler-stale-event.json",
        "contracts/property-coverage/fixtures/regressions/fault-assertion-invalid-continuation.json",
        "contracts/property-coverage/fixtures/regressions/virtio-capacity-status.json",
        "contracts/property-coverage/fixtures/regressions/evidence-complete-tampered.json",
      ],
      freshness = "Each receipt binds the selected profile, named seeds, finite sequence and shrink bounds, suite counts, rejection counts, claim boundary, and minimized counterexamples when present.",
      rationale = "Nickel owns reviewable bounded profiles while Rust owns generation, reference models, execution, shrinking, and observed counterexamples.",
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
