# Human-authored operator receipt over retained Rust runtime/gate facts.
# Raw execution records and the large parent stay local; no workload is promoted.
let DogfoodReceipt = import "../../contracts/evidence/dogfood-receipt.ncl" in
let Runtime = import "runtime-summary.json" in
let Source = import "working-tree-source.ncl" in
let Attempts = import "attempts.ncl" in
let Matrix = import "kvm-release.json" in
let Native = import "native-readiness.json" in
let Coverage = import "traceability.json" in
let Validation = import "native-validation.json" in
let Proposal = import "lineage-proposal-gate.json" in
let Design = import "lineage-design-gate.json" in
let Tasks = import "lineage-tasks-gate.json" in
let First = std.array.at 0 Runtime.replays in
let Second = std.array.at 1 Runtime.replays in
let Stats = First.assertion_coverage in
let ProofAttempt = std.array.at 5 Attempts.attempts in
let ParentPath = Runtime.replay_parent_directory ++ "/" ++ Runtime.replay_parent.path in
{
  schema = "chaoscontrol.production-readiness-receipt.v1",
  owner = "chaoscontrol-vmm",
  production_ready | Bool = false,
  outcome = "blocked",
  scope = "Local Rust Raft/KVM continuation and repository-gate observations for the bound dirty source; not universal determinism or release eligibility",
  immediate_outcome = "Repaired incremental lineage and exercised a persisted depth-four parent in two equal faulted continuations; required native/clean-source readiness still fails",
  durable_capability = "Bounded pure page-set planning, exact incremental capture/revert and base handoffs, plus a repeatable real Raft lineage/continuation proof",
  current_consumers = ["explorer frontier branching", "fresh-controller restore and subsequent incremental capture"],
  source = Source,
  implementation_attempts = Attempts,
  evidence_artifacts = [
    { path = "dogfood-results/production-readiness-20261015/attempts.ncl", digest = "blake3:405bb89ef7179d705f297fe6d22947fe8978538db201078aa98b90e87d562bdf" },
    { path = "dogfood-results/production-readiness-20261015/working-tree-source.ncl", digest = "blake3:87792d9bde33b99823e9b20986f41da9c8f7b0550869031bf1ed08df43b15bbb" },
    { path = "dogfood-results/production-readiness-20261015/raft-campaigns.ncl", digest = "blake3:3f08fe7ea64281c7a10cb0d65dee658bd9da70f6d4690bd618d66c0802951ef9" },
    { path = "dogfood-results/production-readiness-20261015/campaign-initial-observations.json", digest = "blake3:cb7e190edfbcf08d2698ddccedf4f5aeeafa25434fdd80b6b73ef02adf38f70f" },
    { path = "dogfood-results/production-readiness-20261015/campaign-observations.json", digest = "blake3:54f0ece1c7f3c17ff1b6c25a0d0465a03ecb820a0011060ce2f3e8a4f0b7ab0a" },
    { path = "dogfood-results/production-readiness-20261015/runtime-summary.json", digest = "blake3:4869734c7b397e56f14b9b134ed2b5e74bf26a572257c3435810b84d1fa1a109" },
    { path = "dogfood-results/production-readiness-20261015/kvm-release.json", digest = "blake3:f997c55dcb6fdde152d22cb8bf8f9bd69cee238ca59069c57b6f69e65adafaad" },
    { path = "dogfood-results/production-readiness-20261015/native-readiness.json", digest = "blake3:523dafec996cfd2a30fd29b76f96a44df314af8fc5ba4d6e682e30af27feda27" },
    { path = "dogfood-results/production-readiness-20261015/traceability.json", digest = "blake3:8d7d7c2e2ed878eca43233886d47a270c47595143bfa5b19c450d29e36ce50a1" },
    { path = "dogfood-results/production-readiness-20261015/native-validation.json", digest = "blake3:15c4ecb557b989c7c5d4dc26e618f5d2ce00a49ea660bff71b0c517a52dfe1b7" },
    { path = "dogfood-results/production-readiness-20261015/lineage-proposal-gate.json", digest = "blake3:45608306866ba592f6a5f30ccc2001dafe9426828d6918635577808c553d699c" },
    { path = "dogfood-results/production-readiness-20261015/lineage-design-gate.json", digest = "blake3:d74fba70dddd9ef909e855aa2db1c51d7099360524d374f8cfebd36b70854387" },
    { path = "dogfood-results/production-readiness-20261015/lineage-tasks-gate.json", digest = "blake3:cd1a480839bbedad2002c522eea241d778e3ecd7c2fee15d6078e3be5fc49982" },
    { path = ".cairn/changes/repair-incremental-snapshot-lineage/tasks.md", digest = "blake3:f8240c10c5ee5da57bbc529800c268ca1808a01b8c896e4757fd89d83d08ead8" },
    { path = "docs/snapshot-fidelity.md", digest = "blake3:1dc591f25be0db0c2ad42dbc88d96b804f000e34fe69a59a841e6132a7073e62" },
  ],
  observation | DogfoodReceipt = {
    schema_version = "chaoscontrol.production-continuation-observation.v1",
    status = "known-gap",
    acceptance_status = "known-gap",
    git_revision = Source.git_revision,
    run_id = "production-readiness-20261015-raft-lineage-3",
    command = ProofAttempt.after.command,
    config = Runtime.profile_artifact,
    kernel_path = Runtime.profile.kernel.path,
    initrd_path = Runtime.profile.initrd.path,
    artifact_hashes = std.array.concat
      [Runtime.profile.kernel, Runtime.profile.initrd,
       { path = ParentPath, sha256 = Runtime.replay_parent.digest }]
      (std.array.concat First.artifacts Second.artifacts),
    assertion_coverage = {
      registered = Stats.catalog_size,
      exercised = Stats.catalog_size - Stats.unexercised,
      passed = Stats.passed,
      failed = Stats.failed,
      unexercised = Stats.unexercised,
      summary_path = "dogfood-results/production-readiness-20261015/runtime-summary.json",
    },
    bug_reports = [],
    checkpoint_reference = {
      path = ParentPath,
      digest = Runtime.replay_parent.digest,
      kernel_path = Runtime.profile.kernel.path,
      initrd_path = Runtime.profile.initrd.path,
      seed = Runtime.profile.seed,
    },
    raw_logs = [],
  },
  runtime = {
    summary = "dogfood-results/production-readiness-20261015/runtime-summary.json",
    full_local_record = Runtime.runtime_proof,
    profile = Runtime.profile,
    snapshots = Runtime.snapshots,
    retained_parent = Runtime.replay_parent,
    full_restore_incremental_recapture = Runtime.full_restore_incremental_recapture,
    passed = Runtime.passed,
    all_execution_digests_equal = First.execution_traces == Second.execution_traces,
    all_admitted_assertion_outcomes_equal = First.all_admitted_assertion_outcomes_digest == Second.all_admitted_assertion_outcomes_digest,
    round_observations_equal = First.rounds_digest == Second.rounds_digest,
    fault_ledgers_equal = First.fault_ledger == Second.fault_ledger,
    safety_invariants_hold = Runtime.safety_invariants_hold,
    continuations = std.array.map (fun replay => {
      name = replay.name,
      parent_tick = replay.parent_tick,
      end_tick = replay.end_tick,
      rounds = replay.round_count,
      execution_traces = replay.execution_traces,
      all_admitted_assertion_outcomes_digest = replay.all_admitted_assertion_outcomes_digest,
      round_observations_digest = replay.rounds_digest,
      assertion_hits_in_interval = replay.assertion_hits_in_interval,
      assertion_coverage = replay.assertion_coverage,
      fault_observed = replay.fault_observed,
      fault_counters = replay.fault_ledger.counters,
    }) Runtime.replays,
    replay_class = "bounded-snapshot-continuation; no selected bug verdict",
    assertion_boundary = "All 44 identities in the merged and each per-VM report were compared, including failed and unexercised records. Seven merged sites passed; four bounded sometimes sites failed identically; 33 were unexercised. This is not an all-assertions-passing or all-sites-covered claim.",
    trace_boundary = "Strict existing DlogRecord determinism fields with exit/tick/sequence completeness checks. Not CanonicalTrace::identity or a complete instruction/register trace.",
  },
  nickel_validation = {
    command = "/nix/store/ck3a4k9az87z5rjwgy8nmpcs34xp9cc5-nickel-lang-rev-fixup-1.17.0/bin/nickel export --format json dogfood-results/production-readiness-20261015/receipt.ncl",
    evaluator_version = "nickel-lang-cli nickel 1.17.0 (rev 1320a98)",
    executable_digest = "blake3:7f456b69a70cdcbdc32aa05edb8ed1f143493eba5b69e6417367180b2049e2cf",
    contract = "contracts/evidence/dogfood-receipt.ncl",
    boundary = "Receipt and dispatched ten-round inputs were validated with the exact admitted evaluator. The older Eval PATH evaluator was not used as cohort authority; its exported values were checked equal.",
  },
  repository_gates = {
    cargo = {
      command = "nix develop -c cargo test --offline --locked --workspace --all-targets && nix develop -c cargo clippy --offline --locked --workspace --all-targets -- -D warnings && nix develop -c cargo fmt --all -- --check",
      pueue_task = 32,
      pueue_label = "cc-production-final-cargo",
      exit_code = 0,
      outcome = "passed",
    },
    nix = {
      command = "nix flake check --no-update-lock-file --builders '' --option secret-key-files '' -L",
      pueue_task = 28,
      pueue_label = "cc-production-final-source-nix",
      started = "2026-10-15T18:32:06-04:00",
      finished = "2026-10-15T18:35:43-04:00",
      exit_code = 0,
      outcome = "passed",
      flake_checks = 67,
      rust_source = Source.nix_rust_source,
      prior_full_run = { pueue_task = 25, pueue_label = "cc-production-final-nix-scope-fixed", exit_code = 0 },
      boundary = "Final Rust/portable check cohort, not clean-source release eligibility or fresh remote-source availability. The completed receipt/configuration records are validated separately.",
    },
    native_cairn = {
      executable = "/nix/store/bwjb9hzl3xrndwxbg48ck624pdyakf3p-cairn-0.1.0/bin/cairn",
      source_revision = "b6759012db4dab0158936e995b7bad39ac10465b",
      source = "/nix/store/nh55gw93v3xdks6bg2h9yq3z44sw18f3-source",
      policy_freshness = {
        command = "policy export --source cairn-policy/default.ncl --output cairn-policy/generated/cairn-policy.json --check",
        exit_code = 0,
        outcome = "policy fresh: cairn-policy/generated/cairn-policy.json",
      },
      validation = {
        command = "validate --root .",
        exit_code = 0,
        valid = Validation.valid,
        artifact = "dogfood-results/production-readiness-20261015/native-validation.json",
      },
      structural_gates = {
        proposal = { command = "gate proposal repair-incremental-snapshot-lineage --root .", valid = Proposal.valid, verdict = Proposal.verdict },
        design = { command = "gate design repair-incremental-snapshot-lineage --root .", valid = Design.valid, verdict = Design.verdict },
        tasks = { command = "gate tasks repair-incremental-snapshot-lineage --root .", valid = Tasks.valid, verdict = Tasks.verdict },
        boundary = "Structural/advisory pass, not completed implementation, acceptance, sync, archive, or release authority. Three implementation/evidence tasks remain unchecked.",
      },
      traceability = {
        command = "traceability coverage --root . --json",
        valid = Coverage.valid,
        verdict = Coverage.verdict,
        requirements = Coverage.requirements,
        referenced = Coverage.referenced,
        missing = std.array.length Coverage.missing,
        dangling = std.array.length Coverage.dangling,
        artifact = "dogfood-results/production-readiness-20261015/traceability.json",
      },
      readiness = {
        command = "release-readiness --root .",
        exit_code = 0,
        valid = Native.valid,
        verdict = Native.verdict,
        onix_release_ready = Native.onix_release_ready,
        failed_checks = ["traceability_coverage", "mcp_agent_smoke", "preserves_corpus_freshness"],
        artifact = "dogfood-results/production-readiness-20261015/native-readiness.json",
        boundary = "Retained non-passing native readiness record; exit zero does not turn valid=false into success. Upstream fixtures were not copied and required checks were not bypassed.",
      },
      ambient_cli_boundary = "The installed newer CLI cannot load this vendored policy (missing outcome-machine). The matching pinned cohort above validates it; the active policy-refresh change remains separate.",
    },
    kvm_release = {
      command = "nix develop -c cargo run --offline --locked -p chaoscontrol-evidence --bin run-kvm-release-matrix -- --root . --matrix contracts/kvm-release/matrix.json --out target/production-readiness-20261015/kvm-release --expected-revision 413713245198b19a80a62f0bcd4498480fd0d984",
      pueue_task = 22,
      terminal_class = Matrix.terminal_class,
      source = Matrix.source,
      rows = std.array.map (fun row => { id = row.id, status = row.status, exit_code = row.exit_code }) Matrix.rows,
      artifact = "dogfood-results/production-readiness-20261015/kvm-release.json",
      blocker = "All seven required rows passed; the matrix correctly remains blocked because source was dirty. Commits were not authorized.",
    },
  },
  campaign_observations = {
    initial = "dogfood-results/production-readiness-20261015/campaign-initial-observations.json",
    initial_scope = "Five-round control and planted runs each stopped after one round/two branches with eight depth-one sometimes records. No safety finding or deep replay is claimed from these runs.",
    ten_round = "dogfood-results/production-readiness-20261015/campaign-observations.json",
    ten_round_scope = "Both cases timed out at 600 seconds after emitting round-three metrics. Their last valid checkpoints retain two rounds/three branches, eight depth-one sometimes records, and no retained safety finding. No depth-four control completion or deep finding replay is claimed.",
    admitted_inputs = "dogfood-results/production-readiness-20261015/raft-campaigns.ncl",
    input_boundary = "The current admitted ten-round profile corrects the existing short-run first-finding threshold. Declared inputs do not prove execution or campaign completion.",
  },
  replay_attempts = [
    { pueue_task = 26, pueue_label = "cc-production-raft-lineage-proof", output = "target/production-readiness-20261015/raft-lineage-1", exit_code = 124, outcome = "timeout; not accepted" },
    { pueue_task = 29, pueue_label = "cc-production-raft-lineage-proof-optimized", output = "target/production-readiness-20261015/raft-lineage-2", exit_code = 1, outcome = "empty fault ledger; not accepted" },
    { pueue_task = 31, pueue_label = "cc-production-attempt6-restored-fault-run", output = "target/production-readiness-20261015/raft-lineage-3", exit_code = 0, outcome = "passed bounded two-continuation comparison; no selected bug verdict" },
  ],
  known_gaps = std.array.concat Runtime.gaps [
    "Strict clean-source release eligibility is impossible with the current retained edits and the no-commit boundary; the gate was not relaxed.",
    "Native readiness still fails declared linkage (226 missing of 382 and five dangling references) and unconditional Cairn self-fixture/corpus checks. The pinned and current local Cairn sources inspected provide no configurable downstream repair.",
    "The native report also advertises a Cairn-owned Slotcar command and omits explicit determinism replay; those are not represented as executed ChaosControl gates.",
    "The active repair remains open for a real watchdog replacement event, deep exploration-finding replay, and its remaining campaign/design evidence, including unrun redb overlay measurements.",
    "The ten-round control and planted campaigns both timed out operationally at 600 seconds. Their partial checkpoints do not satisfy the active repair's deep-campaign acceptance conditions.",
    "Pre-repair deep findings stay diagnostic-only. No accepted-workload manifest, generated promotion status, or workload support claim was changed.",
    "Nix evaluation used cached Git refs after HTTP 530 responses. The successful local cached build does not prove remote-source availability. kache reported disabled caching; compilation still passed.",
    "Pueue numeric IDs can be reused; pair them with exact command, unique label, and recorded output rather than treating a number as durable evidence identity.",
  ],
}
