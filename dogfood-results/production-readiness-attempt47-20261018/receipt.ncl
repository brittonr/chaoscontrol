# This scoped receipt preserves every original production criterion and failed gate.
let P = import "../../contracts/evidence/profile-primitives.ncl" in
let O = import "observations.json" in
let M = import "evidence-manifest.json" in
let Attempts = import "attempts.ncl" in
let Prior = import "../production-readiness-attempt46-20261018/receipt.ncl" in
let Artifact = { path | String, digest | P.blake3, bytes | P.u64 } in
let RequiredGateIds = ["workspace-tests", "clippy", "formatting", "watchdog-proof",
  "native-gates", "nix-checks", "kvm-release"] in
let CompleteGateInventory = std.array.length O.gates == std.array.length RequiredGateIds
  && P.unique_by (fun gate => gate.id) O.gates
  && std.array.all (fun id => std.array.any (fun gate => gate.id == id) O.gates) RequiredGateIds in
let GatePassed = fun id => std.array.any (fun gate => gate.id == id
  && gate.executed && gate.source_revision == O.source.revision
  && gate.exit_code == 0 && !gate.operational_timeout
  && gate.report_passed && gate.passed) O.gates in
let CargoPassed = std.array.all GatePassed ["workspace-tests", "clippy", "formatting"] in
let RegressionPassed = O.correction.failing_before_reproduced
  && O.correction.failing_before.exit_code == 101
  && !O.correction.failing_before.operational_timeout
  && O.correction.passing_after_verified
  && O.correction.passing_after.exit_code == 0
  && !O.correction.passing_after.operational_timeout in
let CompletedCampaign = fun campaign => campaign.valid_campaign_output
  && campaign.rounds_completed == 10 && campaign.total_branches_run == 20
  && !campaign.command_observation.operational_timeout
  && campaign.command_observation.timeout_seconds == 600
  && std.array.elem campaign.command_observation.exit_code [0, 1] in
let ControlAndPlantedComplete = std.array.all (fun id => std.array.any (fun campaign =>
  campaign.id == id && CompletedCampaign campaign) O.campaigns) ["control", "planted"] in
let MeasuredDepthFour = fun campaign => CompletedCampaign campaign
  && campaign.capture_alignment_verified && campaign.maximum_admitted_depth >= 4
  && std.array.all (fun depth => std.array.any (fun capture =>
    capture.child_depth == depth && capture.overlay_payload_bytes > 0)
    campaign.overlay_captures) [1, 2, 3, 4] in
let CampaignDepthFourPassed = std.array.all (fun id => std.array.any (fun campaign =>
  campaign.id == id && MeasuredDepthFour campaign) O.campaigns) ["control", "planted"] in
let Reproduced = fun replay => replay.passed && replay.parent_depth >= 3
  && replay.command_observation.exit_code == 0
  && !replay.command_observation.operational_timeout
  && replay.verdict != null
  && replay.verdict.replay_parent_depth == replay.parent_depth
  && replay.verdict.replay_class == "snapshot_backed_reproduced"
  && replay.verdict.reproduced && replay.verdict.snapshot.status == "valid"
  && replay.verdict.snapshot.present && replay.verdict.snapshot.digest_verified in
let DeepPassed = O.replay_inventory.all_inventories_complete
  && O.replay_inventory.all_retained_deep_safety_findings_replayed
  && std.array.length O.deep_replays > 0
  && std.array.all Reproduced O.deep_replays in
let W = O.watchdog in
let WatchdogPassed = GatePassed "watchdog-proof" && W.proof_present && W.passed
  && W.equal && W.fresh_full_equal && W.watchdog_replacement_equal
  && W.safety_invariants_hold && W.full_restore_incremental_recapture.memory_equal
  && W.watchdog_recovery.passed && W.watchdog_recovery.watchdog_observed
  && W.watchdog_recovery.timer_signals_observed > 0
  && W.watchdog_recovery.poison_enforced && W.watchdog_recovery.replacement_created
  && W.watchdog_recovery.restored_parent_memory_equal
  && W.watchdog_recovery.immediate_incremental_recapture.memory_equal
  && W.watchdog_recovery.classification == "operational-host-watchdog-timeout"
  && !W.watchdog_recovery.guest_bug
  && std.array.all (fun field => std.record.get field W.strict_comparison_fields)
    ["initial_memory_identities", "final_memory_identities", "execution_traces",
      "all_admitted_assertion_outcomes", "fault_ledger", "rounds"]
  && std.array.all (fun depth => std.array.any (fun snapshot => snapshot.depth == depth
    && snapshot.full_reference_memory_equal && snapshot.dirty_pages > 0
    && snapshot.overlay_payload_bytes > 0) W.snapshots) [1, 2, 3, 4] in
let CurrentEvidenceVerified = M.identity_algorithm == "blake3"
  && M.all_current_manifest_inputs_verified
  && M.all_historical_receipt_bytes_unchanged && Attempts.within_limit in
let FocusedCorrectionVerified = RegressionPassed && CargoPassed
  && ControlAndPlantedComplete && CampaignDepthFourPassed && DeepPassed
  && CurrentEvidenceVerified in
let SelectedOriginalPassed = O.selected_original_current_source_attempt.executed
  && O.selected_original_current_source_attempt.passed in
let FreshReleasePassed = CompleteGateInventory && !O.source.dirty
  && std.array.all GatePassed RequiredGateIds in
let EvidenceRetained = CurrentEvidenceVerified && M.all_historical_required_artifacts_retained in
let Ready = FocusedCorrectionVerified && CampaignDepthFourPassed && DeepPassed
  && SelectedOriginalPassed && WatchdogPassed && FreshReleasePassed && EvidenceRetained in
{
  schema = "chaoscontrol.production-readiness-focused-attempt47-receipt.v1",
  owner = "chaoscontrol-vmm",
  maintenance_owner = "chaoscontrol-vmm",
  immediate_outcome = O.immediate_outcome,
  durable_capability = "Bounded logical-marker proposal multiplicity with distinct owner/context preservation, rarity-hit retention, unchanged Campaign decisions, and exact-source campaign/replay/watchdog evidence",
  current_consumer = "ChaosControl explorer frontier construction for Campaign and legacy-diagnostic backends",
  adoption_path = "Existing Explorer::entry_proposals path shared by both frontier backends; no Campaign selection-policy change, validation bypass or compatibility shim",
  production_ready | std.contract.Equal Ready = Ready,
  outcome = if Ready then "ready" else "blocked",
  focused_correction_verified = FocusedCorrectionVerified,
  source = O.source,
  source_archive | Artifact = O.source_archive,
  source_archive_command = O.source_archive_command,
  retained_runtime_inputs = O.retained_runtime_inputs,
  helper_sources_archive | Artifact = O.helper_sources_archive,
  original_acceptance_criteria_unchanged | std.contract.Equal true = true,
  implementation_attempts = {
    original_maximum | std.contract.Equal 20 = Attempts.original_maximum_implementation_attempts,
    authorized_maximum | std.contract.Equal 47 = Attempts.maximum_implementation_attempts,
    used | std.contract.Equal 47 = Attempts.used_implementation_attempts,
    within_limit | std.contract.Equal true = Attempts.within_limit,
    authorization = Attempts.authorization,
    counting_rule = Attempts.counting_rule,
    attempts = Attempts.attempts,
  },
  complete_gate_inventory | std.contract.Equal true = CompleteGateInventory,
  required_criteria = [
    { id = "native-cairn-readiness", passed = GatePassed "native-gates" },
    { id = "deep-raft-campaign-and-replay", passed = CampaignDepthFourPassed && DeepPassed && SelectedOriginalPassed },
    { id = "strict-watchdog-continuation", passed = WatchdogPassed },
    { id = "fresh-release-gates", passed = FreshReleasePassed },
    { id = "reproducible-evidence-retention", passed = EvidenceRetained },
  ],
  regression = {
    failing_before_reproduced = O.correction.failing_before_reproduced,
    passing_after_verified = O.correction.passing_after_verified,
    invariant = O.correction.invariant,
    prior_read_only_diagnosis | Artifact = O.correction.historical_diagnosis,
  },
  campaigns = {
    completed_control_and_planted_under_original_bounds = ControlAndPlantedComplete,
    complete_control_and_planted_depth_four = CampaignDepthFourPassed,
    observations = std.array.map (fun campaign => {
      id = campaign.id,
      rounds = campaign.rounds_completed,
      branches = campaign.total_branches_run,
      elapsed_seconds = campaign.command_observation.elapsed_seconds,
      operational_timeout = campaign.command_observation.operational_timeout,
      valid_campaign_output = campaign.valid_campaign_output,
      maximum_admitted_depth = campaign.maximum_admitted_depth,
      overlay_captures = campaign.overlay_captures,
      overlay_sizes_by_depth = campaign.overlay_sizes_by_depth,
      capture_alignment_verified = campaign.capture_alignment_verified,
      assertion_totals = campaign.assertion_totals,
      retained_safety_findings = campaign.retained_safety_findings,
      liveness_failures = campaign.liveness_failures,
      host_watchdog_timeouts = campaign.host_watchdog_timeouts,
    }) O.campaigns,
  },
  deep_replays = {
    all_current_retained_deep_safety_findings_replayed = DeepPassed,
    observations = std.array.map (fun replay => {
      campaign_id = replay.campaign_id,
      bug_id = replay.bug_id,
      parent_depth = replay.parent_depth,
      passed = replay.passed,
      bug | Artifact = replay.bug,
      verdict = replay.verdict_artifact,
    }) O.deep_replays,
    selected_original_current_source_attempt = O.selected_original_current_source_attempt,
  },
  watchdog = W,
  release_gates = O.gates,
  workspace_totals = O.workspace_totals,
  prior_source_revision = O.prior_source_revision,
  native_traceability = O.native_traceability,
  prior_focused_source_revision = O.prior_focused_source_revision,
  prior_native_verdict = Prior.prior_native_verdict,
  upstream_prerequisite = Prior.upstream_prerequisite,
  upstream_observation = O.upstream_observation,
  prior_kvm_verdict = Prior.prior_kvm_verdict,
  historical_missing_artifact = O.historical_missing_artifact,
  historical_preservation = O.historical_receipts,
  observation_records | Artifact = M.observation_records,
  evidence_artifacts | Array Artifact = M.artifacts,
  current_evidence_verified = CurrentEvidenceVerified,
  original_historical_artifact_inventory_retained = M.all_historical_required_artifacts_retained,
  blockers = O.blockers,
  non_claims = O.non_claims,
}
