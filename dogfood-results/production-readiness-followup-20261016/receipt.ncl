# Receipt over retained runtime facts and exact clean-source release observations.
# Successful commands cannot replace passing product reports or missing artifacts.
let P = import "../../contracts/evidence/profile-primitives.ncl" in
let O = import "release-observations.json" in
let Attempts = import "attempts.ncl" in
let Campaigns = import "campaign23-observations.json" in
let Deep = import "deep-probe24-observations.json" in
let Runtime = import "runtime21-observations.json" in
let NativeConsumer = import "native-consumer-observations.json" in
let LocalFix = import "local-kvm25-observations.json" in
let MatrixProfile = import "../../contracts/kvm-release/matrix.json" in
let Artifact = { path | String, digest | P.blake3, bytes | P.u64 } in
let Gate = {
  id | String,
  passed | Bool,
  exit_code | P.nullable_u32,
  executed | Bool,
  operational_timeout | Bool,
  report_passed | Bool,
  observation | Artifact,
} in
let Native = O.native_cairn in
let NativePassed = Native.validation.valid
  && std.array.length Native.validation.issues == 0
  && std.array.length Native.validation.findings == 0
  && Native.traceability.valid
  && Native.traceability.verdict == "pass"
  && std.array.length Native.traceability.missing == 0
  && std.array.length Native.traceability.dangling == 0
  && Native.traceability.referenced == Native.traceability.requirements
  && Native.readiness.valid
  && Native.readiness.verdict == "pass"
  && Native.readiness.coverage_audit_valid
  && std.array.length Native.readiness.checks > 0
  && std.array.all (fun check => check.passed) Native.readiness.checks
  && std.array.all (fun name =>
    std.array.any (fun check => check.name == name && check.passed
      && (!(std.record.has_field "skipped" check) || !check.skipped)) Native.readiness.checks
  ) ["mcp_agent_smoke", "preserves_corpus_freshness"]
  && Native.summary.all_required_passed in
let MeasuredDepthFour = fun campaign =>
  campaign.valid_campaign_output && campaign.rounds_completed == 10
  && campaign.command_observation.exit_code == 0
  && !campaign.command_observation.operational_timeout
  && campaign.command_observation.timeout_seconds == 600
  && campaign.maximum_admitted_depth >= 4
  && std.array.all (fun depth =>
    std.array.any (fun capture => capture.child_depth == depth)
      campaign.overlay_captures
  ) [1, 2, 3, 4]
in
let CampaignsPassed = std.array.all (fun id =>
  std.array.any (fun campaign => campaign.id == id && MeasuredDepthFour campaign)
    Campaigns.campaigns
) ["control23", "planted23"] in
let DeepPassed = Deep.passed && Deep.valid_campaign_output
  && Deep.rounds_completed == 10
  && !Deep.campaign_command_observation.operational_timeout
  # A completed explorer run with retained findings returns 1 (CLI lines 1285-1287).
  # Validated output and every required replay remain mandatory below.
  && Deep.campaign_command_observation.exit_code == 1
  && Deep.campaign_command_observation.timeout_seconds == 600
  && Deep.host_watchdog_timeouts == 0
  && Deep.all_retained_deep_safety_findings_replayed
  && (Deep.retained_deep_safety_findings | P.positive_u64) > 0
  && std.array.length Deep.replays == Deep.retained_deep_safety_findings
  && std.array.all (fun replay => replay.passed && replay.parent_depth >= 3
    && replay.verdict.replay_parent_depth == replay.parent_depth
    && replay.verdict.replay_class == "snapshot_backed_reproduced"
    && replay.verdict.reproduced
    && replay.verdict.snapshot.status == "valid"
    && replay.verdict.snapshot.present
    && replay.verdict.snapshot.digest_verified
  ) Deep.replays in
let WatchdogPassed = Runtime.passed && Runtime.equal
  && Runtime.fresh_full_equal && Runtime.watchdog_replacement_equal
  && Runtime.safety_invariants_hold
  && Runtime.watchdog_recovery.passed && Runtime.watchdog_recovery.watchdog_observed
  && Runtime.watchdog_recovery.poison_enforced
  && Runtime.watchdog_recovery.replacement_created
  && !Runtime.watchdog_recovery.guest_bug
  && std.array.all (fun field => std.record.get field Runtime.strict_comparison_fields)
    ["initial_memory_identities", "final_memory_identities", "execution_traces",
      "all_admitted_assertion_outcomes", "fault_ledger", "rounds"]
  && O.clean_watchdog.passed in
let LocalFixPassed = LocalFix.report_verdict_correction_verified
  && LocalFix.actual_cli_negative.expected_failure_observed
  && LocalFix.actual_cli_negative.command_observation.exit_code == 1
  && !LocalFix.actual_cli_negative.command_observation.operational_timeout
  && LocalFix.actual_cli_negative.metadata.exit_code == 1
  && LocalFix.actual_cli_negative.metadata.scheduler_exit_code == 0
  && LocalFix.actual_cli_negative.metadata.scheduler_observation.disposition == "succeeded"
  && LocalFix.actual_cli_negative.campaign_status == "failed" in
let Gates = O.gates | Array Gate in
let RequiredGateIds = ["clean-workspace-tests", "clean-clippy", "clean-formatting",
  "clean-nix-checks", "clean-watchdog-proof", "clean-native-gates", "clean-kvm-release"] in
let CompleteGateInventory = std.array.length Gates == std.array.length RequiredGateIds
  && P.unique_by (fun gate => gate.id) Gates
  && std.array.all (fun id => std.array.any (fun gate => gate.id == id) Gates) RequiredGateIds in
let ReleaseGateIds = ["clean-workspace-tests", "clean-clippy", "clean-formatting",
  "clean-nix-checks", "clean-kvm-release"] in
let ReleasePassed = CompleteGateInventory
  && LocalFixPassed
  && std.array.all (fun id => std.array.any (fun gate => gate.id == id
    && gate.executed && gate.passed && gate.report_passed && gate.exit_code == 0
    && !gate.operational_timeout) Gates) ReleaseGateIds
  && O.local_multi_hypervisor.passed in
let MatrixPassed = O.kvm_release.receipt_present
  && O.kvm_release.terminal_class == "release-eligible"
  && O.kvm_release.source.revision == O.source.revision
  && !O.kvm_release.source.dirty
  && std.array.length O.kvm_release.rows == std.array.length MatrixProfile.rows
  && P.unique_by (fun row => row.id) O.kvm_release.rows
  && std.array.all (fun expected => std.array.any (fun actual =>
    actual.id == expected.id && actual.status == "passed"
  ) O.kvm_release.rows) MatrixProfile.rows in
let EvidenceRetained = O.historical_preservation.all_historical_files_unchanged
  && Attempts.within_limit
  && O.evidence_retention.identity_algorithm == "blake3"
  && O.evidence_retention.all_manifest_inputs_identity_checked
  && O.evidence_retention.all_current_required_artifacts_retained
  && O.evidence_retention.all_historical_required_artifacts_retained in
let Ready = NativePassed && CampaignsPassed && DeepPassed
  && Campaigns.selected_planted_probe.passed && WatchdogPassed
  && ReleasePassed && MatrixPassed && EvidenceRetained in
{
  schema = "chaoscontrol.production-readiness-authorized-followup-receipt.v1",
  owner = "chaoscontrol-vmm",
  production_ready | std.contract.Equal Ready = Ready,
  outcome = if Ready then "ready" else "blocked",
  immediate_outcome = O.immediate_outcome,
  durable_capability = "Strict depth-four watchdog/controller equivalence, measured deep Raft exploration with every retained deep finding replayed, and report-verdict-aware local KVM release accounting",
  maintenance_owner = "chaoscontrol-vmm",
  current_consumers = ["explorer frontier branching", "retained-parent reproduction", "production controller replacement", "local KVM smoke operators"],
  source = O.source,
  source_archive | Artifact = O.source_archive,
  evidence_only_delivery_boundary = "Commands test the exact clean source revision above. Later receipt/documentation-only delivery commits are not represented as the tested source revision.",
  implementation_attempts = {
    original_maximum | std.contract.Equal 20 = Attempts.original_maximum_implementation_attempts,
    authorized_maximum | std.contract.Equal 25 = Attempts.maximum_implementation_attempts,
    used | P.integer_between 20 25 = Attempts.used_implementation_attempts,
    within_limit | std.contract.Equal true = Attempts.within_limit,
    authorization = Attempts.authorization,
    counting_rule = Attempts.counting_rule,
    ledger = "dogfood-results/production-readiness-followup-20261016/attempts.ncl",
  },
  required_criteria = [
    { id = "native-cairn-readiness", passed = NativePassed },
    { id = "deep-raft-campaign-and-replay", passed = CampaignsPassed && DeepPassed && Campaigns.selected_planted_probe.passed },
    { id = "strict-watchdog-continuation", passed = WatchdogPassed },
    { id = "fresh-release-gates", passed = ReleasePassed && MatrixPassed },
    { id = "reproducible-evidence-retention", passed = EvidenceRetained },
  ],
  native_cairn = Native,
  upstream_producer_prerequisite = NativeConsumer.producer_prerequisite,
  campaign_validation = {
    complete_control_and_planted_depth_four = CampaignsPassed,
    complete_records = "dogfood-results/production-readiness-followup-20261016/campaign23-observations.json",
    deep_exploration_replay_satisfied = DeepPassed,
    retained_deep_safety_findings = Deep.retained_deep_safety_findings,
    deep_replays = Deep.replays,
    deep_records = "dogfood-results/production-readiness-followup-20261016/deep-probe24-observations.json",
    selected_planted_probe = Campaigns.selected_planted_probe,
    exact_inputs = "dogfood-results/production-readiness-followup-20261016/raft-campaigns.ncl",
    timeout_diagnosis = O.timeout_diagnosis,
    non_claim = "The late snapshot probe validates lineage mechanics, not an independently discovered Raft algorithm defect. Control, liveness, safety and operational results remain distinct.",
  },
  watchdog = O.clean_watchdog,
  strict_runtime_records = "dogfood-results/production-readiness-followup-20261016/runtime21-observations.json",
  release_gates = Gates,
  kvm_release = O.kvm_release,
  local_multi_hypervisor = O.local_multi_hypervisor,
  local_gate_correction = LocalFix,
  command_observations = O.command_observations,
  artifact_commands = O.artifact_commands,
  prior_clean_release = O.prior_clean_release,
  operational_failure_diagnosis = O.operational_failure_diagnosis,
  evidence_artifacts | Array Artifact = O.evidence_artifacts,
  evidence_retention = O.evidence_retention,
  historical_preservation = O.historical_preservation,
  repeatability = O.repeatability,
  blockers = O.blockers,
  non_claims = O.non_claims,
}
