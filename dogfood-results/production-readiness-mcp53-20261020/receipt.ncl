# MCP correction evidence receipt. Export success is not production readiness.
let P = import "../../contracts/evidence/profile-primitives.ncl" in
let Prior = import "../production-readiness-attempt52-20261020/receipt.ncl" in
let Attempts = import "attempts.ncl" in
let O = import "observations.json" in
let Artifact = { path | String, digest | P.blake3, bytes | P.u64 } in
let Source = O.source | {
  revision | std.contract.Equal "ef778be7ecc0467f176857dac779a262b9b741d1",
  dirty | std.contract.Equal false,
  all_gate_source_checks_unchanged_and_clean | std.contract.Equal true,
} in
let C = O.cairn_cohort in
let CohortReady =
  C.published
  && C.remote_head_verified
  && std.array.length C.gate_rows == 10
  && std.array.all (fun row => row.exit_code == 0) C.gate_rows
  && C.validation_valid
  && C.traceability_valid && C.traceability_verdict == "pass"
  && C.traceability_missing == 0 && C.traceability_dangling == 0
  && C.octet_error_findings == 0
  && C.nix_checks_passed
  && C.workspace_failed_tests == 0
  && C.formatting_passed && C.clippy_passed && C.package_cli_verified in
let M = O.policy_migration in
let MigrationReady =
  std.array.length M.imports == 4
  && std.array.all (fun item =>
    item.upstream_revision == C.revision
    && item.destination_identity_verified
  ) M.imports
  && M.typed_selection_active
  && M.name_and_refresh_preserved
  && M.policy_fresh
  && M.adaptation_is_annotation_only in
let K = O.consumer_completion in
let CompletionReady =
  K.valid && K.complete
  && std.array.length K.rows == 4
  && std.array.all (fun row =>
    row.status == "passed" && row.execution_observed && row.teardown_complete
  ) K.rows
  && K.manifest_verified
  && K.process_exit_is_not_the_verdict
  && K.declared_bounds_unchanged
  && K.commands_match_registry
  && K.dependency_identity_count > 0
  && std.array.length K.ignored_dependency_identities == 0
  && std.array.length K.ignored_command_identities == 0 in
let X = O.cli_mcp in
let AdmissionReady =
  X.completion_valid && X.completion_verdict == "valid"
  && X.mcp_valid && X.mcp_verdict == "pass"
  && X.real_failed_check_row == "failed"
  && X.failed_check_completion_valid == false
  && X.failed_check_consumer_verdict == "fail"
  && X.invalid_selections_rejected == 3
  && X.missing_results_rejected
  && X.stale_source_rejected
  && X.selected_policy_used_without_fallback in
let N = O.native in
let TraceabilityReady =
  N.validation_valid
  && N.coverage_valid && N.coverage_verdict == "pass"
  && N.coverage_referenced == N.coverage_requirements
  && N.coverage_missing == 0 && N.coverage_dangling == 0 in
let NativeReady =
  TraceabilityReady
  && N.readiness_valid && N.readiness_verdict == "pass"
  && std.array.all (fun check => check.passed) N.readiness_checks
  && N.preserves_corpus_fresh
  && N.preserves_stale_rejected in
let V = O.kvm in
let KvmRows = [
  "build-release-binaries", "build-raft-guest", "deterministic-smp",
  "serialized-snapshot-replay", "virtio-malformed-input", "admitted-drift",
  "fresh-workload-replay"
] in
let KvmReady =
  V.run_exit_code == 0 && !V.operational_timeout
  && V.terminal_class == "release-eligible"
  && V.independent_terminal_class == "release-eligible"
  && V.source_revision == Source.revision && !V.source_dirty
  && std.array.map (fun row => row.id) V.rows == KvmRows
  && std.array.all (fun row => row.status == "passed") V.rows
  && V.artifact_verification_all_verified
  && V.independent_validation_exit_code == 0 in
let HistoricalReady =
  Prior.reused_campaign_depth_four_ready
  && Prior.retained_deep_replays_ready
  && Prior.selected_original_replay_ready
  && Prior.strict_watchdog_ready
  && Prior.current_evidence_verified
  && O.historical_preservation.all_unchanged
  && O.historical_preservation.raw_artifacts_local in
let EvidenceReady =
  O.evidence_verification.all_verified
  && O.evidence_verification.artifacts_checked == std.array.length O.artifacts
  && std.array.length O.evidence_verification.identity_mismatches == 0
  && Attempts.within_limit
  && Attempts.maximum_implementation_attempts == 53
  && std.array.length O.remaining_blockers == 2 in
let CorrectionReady =
  CohortReady && MigrationReady && CompletionReady && AdmissionReady
  && NativeReady && KvmReady && EvidenceReady in
{
  schema = "chaoscontrol.mcp53-readiness-receipt.v1",
  owner = "chaoscontrol-vmm",
  maintenance_owner = O.maintenance_owner,
  immediate_outcome = O.immediate_outcome,
  durable_capability = "A real downstream Cairn MCP producer/consumer cohort that executes the complete maintained ChaosControl command set, preserves measured tool environments, and rejects stale, failed, invalid or absent completion without fallback",
  current_consumer = "ChaosControl clean-source release gates and the selected policy-native release readiness report",
  source = Source,
  implementation_attempts = {
    original_maximum | std.contract.Equal 20 = 20,
    authorized_maximum | std.contract.Equal 53 = Attempts.maximum_implementation_attempts,
    used | std.contract.Equal 53 = Attempts.used_implementation_attempts,
    within_limit | std.contract.Equal true = Attempts.within_limit,
    authorization = Attempts.authorization,
    attempts = Attempts.attempts,
    execution_attempts = Attempts.execution_attempts,
  },
  original_acceptance_criteria_unchanged | std.contract.Equal true = true,
  cairn_cohort | std.contract.Equal true = CohortReady,
  cairn_cohort_evidence = C,
  policy_migration | std.contract.Equal true = MigrationReady,
  policy_migration_evidence = M,
  consumer_completion | std.contract.Equal true = CompletionReady,
  consumer_completion_evidence = K,
  cli_mcp_admission | std.contract.Equal true = AdmissionReady,
  cli_mcp_evidence = X,
  native_traceability | std.contract.Equal true = TraceabilityReady,
  native_readiness | std.contract.Equal true = NativeReady,
  native_evidence = N,
  kvm_release_matrix | std.contract.Equal true = KvmReady,
  kvm_evidence = V,
  historical_runtime | std.contract.Equal true = HistoricalReady,
  historical_runtime_source = {
    receipt = "../production-readiness-attempt52-20261020/receipt.ncl",
    reused_campaign_depth_four_ready = Prior.reused_campaign_depth_four_ready,
    retained_deep_replays_ready = Prior.retained_deep_replays_ready,
    selected_original_replay_ready = Prior.selected_original_replay_ready,
    strict_watchdog_ready = Prior.strict_watchdog_ready,
    current_evidence_verified = Prior.current_evidence_verified,
  },
  mcp_correction_ready | std.contract.Equal true = CorrectionReady,
  production_ready | std.contract.Equal false = false,
  outcome = if CorrectionReady then "mcp-ready-production-blocked" else "blocked",
  exact_commands = O.commands,
  evidence_verification = O.evidence_verification,
  evidence_artifacts | Array Artifact = O.artifacts,
  historical_preservation = O.historical_preservation,
  remaining_prerequisites = O.remaining_blockers,
  non_claims = O.non_claims,
}
