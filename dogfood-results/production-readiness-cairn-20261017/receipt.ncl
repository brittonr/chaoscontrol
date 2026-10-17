# Separate authorized evidence. Prior gate verdicts are never promoted.
let P = import "../../contracts/evidence/profile-primitives.ncl" in
let O = import "observations.json" in
let A = import "attempts.ncl" in
let Prior = import "../production-readiness-followup-20261016/receipt.json" in
let Artifact = { path | String, digest | P.blake3, bytes | P.u64 } in
let Native = O.native_consumer.reports in
let NativePassed = Native.validation.valid
  && std.array.length Native.validation.issues == 0
  && std.array.length Native.validation.findings == 0
  && Native.traceability.valid && Native.traceability.verdict == "pass"
  && std.array.length Native.traceability.missing == 0
  && std.array.length Native.traceability.dangling == 0
  && Native.traceability.referenced == Native.traceability.requirements
  && Native.readiness.valid && Native.readiness.verdict == "pass"
  && Native.readiness.coverage_audit_valid
  && std.array.length Native.readiness.checks > 0
  && std.array.all (fun check => check.passed) Native.readiness.checks
  && std.array.all (fun name => std.array.any (fun check => check.name == name
    && check.passed && (!(std.record.has_field "skipped" check) || !check.skipped))
    Native.readiness.checks) ["mcp_agent_smoke", "preserves_corpus_freshness"] in
let PriorCriterionPassed = fun name => std.array.any (fun criterion =>
  criterion.id == name && criterion.passed) Prior.required_criteria in
let CurrentEvidenceRetained = O.evidence_identity.checked_carrier_count > 0
  && O.evidence_identity.all_new_carriers_present_and_matching
  && std.array.length O.evidence_identity.failures == 0
  && std.array.all (fun carrier => carrier.present && carrier.matches)
    O.evidence_identity.carriers
  && O.prior_receipts.all_listed_sources_and_receipts_unchanged
  && std.array.all (fun item => item.unchanged) O.prior_receipts.preservation
  && A.within_limit in
let Criteria = [
  { id = "native-cairn-readiness", passed = NativePassed },
  { id = "deep-raft-campaign-and-replay", passed = PriorCriterionPassed "deep-raft-campaign-and-replay" },
  { id = "strict-watchdog-continuation", passed = PriorCriterionPassed "strict-watchdog-continuation" },
  { id = "fresh-release-gates", passed = PriorCriterionPassed "fresh-release-gates" },
  { id = "reproducible-evidence-retention", passed = CurrentEvidenceRetained
      && PriorCriterionPassed "reproducible-evidence-retention" },
] in
let Ready = std.array.all (fun criterion => criterion.passed) Criteria
  && O.policy_migration.installed
  && O.cairn.mechanism_receipt.all_required_checks_passed in
{
  schema = "chaoscontrol.production-readiness-authorized-cairn-receipt.v1",
  owner = "chaoscontrol-vmm",
  production_ready | std.contract.Equal Ready = Ready,
  outcome = if Ready then "ready" else "blocked",
  immediate_outcome = "Producer dependency/Nix mechanisms are verified and published on a dedicated Cairn branch; compatible consumer migration, full MCP completion and production release acceptance remain blocked",
  durable_capability = "Bounded dependency identity observation and native build-payload admission, with original completion/manifest/MCP consumers and explicit independent gate failures",
  maintenance_owner = "Cairn CLI/core maintainers own producer and source-gate prerequisites; ChaosControl owns adoption, product evidence and release acceptance",
  current_consumer = "ChaosControl unchanged required-check completion contract",
  adoption_path = "Obtain the passing immutable Cairn CLI/export cohort, migrate the complete four-file closure through UPSTREAM.md, then execute the full consumer contract with every declared command and dependency",
  source = O.source,
  evidence_only_delivery_boundary = "This receipt/documentation delivery is not represented as a clean-source tested revision. Earlier runtime and release facts retain the exact source and artifact boundaries in the prior receipt.",
  implementation_attempts = {
    original_maximum | std.contract.Equal 20 = A.original_maximum_implementation_attempts,
    authorized_maximum | std.contract.Equal 30 = A.maximum_implementation_attempts,
    used | std.contract.Equal 30 = A.used_implementation_attempts,
    within_limit | std.contract.Equal true = A.within_limit,
    new_attempt_inventory_complete | std.contract.Equal true = std.array.length A.attempts == 5
      && std.array.all (fun number => std.array.length (std.array.filter
        (fun attempt => attempt.number == number) A.attempts) == 1) [26, 27, 28, 29, 30],
    no_planned_attempts_remaining | std.contract.Equal true = std.array.all
      (fun attempt => attempt.status != 'planned) A.attempts,
    authorization = A.authorization,
    counting_rule = A.counting_rule,
    ledger = "dogfood-results/production-readiness-cairn-20261017/attempts.ncl",
  },
  required_criteria = Criteria,
  producer_mechanism_verified | std.contract.Equal true = O.cairn.mechanism_receipt.mechanism_conformance,
  producer_required_gates_passed = O.cairn.mechanism_receipt.all_required_checks_passed,
  full_consumer_mcp_contract_exercised | std.contract.Equal false = O.cairn.mechanism_receipt.consumer_full_contract_exercised,
  cairn = O.cairn,
  native_consumer = O.native_consumer,
  retained_policy_freshness_passed | std.contract.Equal true = O.native_consumer.policy_freshness_passed,
  runtime_policy_and_pins_preserved | std.contract.Equal true = O.policy_migration.all_runtime_policy_and_pin_files_unchanged,
  policy_migration = O.policy_migration,
  requirement_audit = O.requirement_audit,
  remaining_requirements | std.contract.Equal 120 = std.array.length Native.traceability.missing,
  dangling_references | std.contract.Equal 0 = std.array.length Native.traceability.dangling,
  current_evidence_retained | std.contract.Equal true = CurrentEvidenceRetained,
  evidence_identity = O.evidence_identity,
  prior_receipts = O.prior_receipts,
  inherited_execution_evidence = {
    receipt | Artifact = O.prior_receipts.inherited_receipt,
    source = Prior.source,
    required_criteria = Prior.required_criteria,
    campaign_records = Prior.campaign_validation.complete_records,
    deep_replay_records = Prior.campaign_validation.deep_records,
    strict_runtime_records = Prior.strict_runtime_records,
    boundary = "Retains exact faults, snapshots, per-depth overlays, assertion coverage, bug identities, every deep replay, watchdog outcomes and gate commands; no inherited verdict is newly executed or promoted",
  },
  release_gate_boundary = O.release_gate_boundary,
  repeatability = "Use the recorded public immutable source and exact commands, retained executable/source archive, declared inputs and BLAKE3 carrier manifest. Replay and release commands retain their original source/config/artifact identities. Raw logs and VM artifacts remain local.",
  blockers = O.blockers,
  non_claims = O.non_claims,
}
