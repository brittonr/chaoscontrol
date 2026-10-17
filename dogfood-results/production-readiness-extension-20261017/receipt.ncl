# Exact clean-source gate observations, not an exit-only or fixture-copy claim.
let P = import "../../contracts/evidence/profile-primitives.ncl" in
let O = import "observations.json" in
let M = import "evidence-manifest.json" in
let Attempts = import "attempts.ncl" in
let Campaigns = import "../production-readiness-followup-20261016/campaign23-observations.json" in
let Deep = import "../production-readiness-followup-20261016/deep-probe24-observations.json" in
let MatrixProfile = import "../../contracts/kvm-release/matrix.json" in
let Artifact = { path | String, digest | P.blake3, bytes | P.u64 } in
let ExitCode = std.contract.custom (fun label value =>
  if value == null then 'Ok value
  else std.contract.check (P.integer_between (-2147483648) 2147483647) label value
) in
let Gate = {
  id | String,
  executed | Bool,
  exit_code | ExitCode,
  operational_timeout | Bool,
  report_passed | Bool,
  passed | Bool,
  observation | Artifact,
} in
let Gates = O.gates | Array Gate in
let RequiredGateIds = ["workspace-tests", "clippy", "formatting", "nix-checks",
  "watchdog-proof", "native-gates", "kvm-release"] in
let CompleteGateInventory = std.array.length Gates == std.array.length RequiredGateIds
  && P.unique_by (fun gate => gate.id) Gates
  && std.array.all (fun id => std.array.any (fun gate => gate.id == id) Gates) RequiredGateIds in
let GatePassed = fun id => std.array.any (fun gate => gate.id == id
  && gate.executed && gate.passed && gate.report_passed
  && gate.exit_code == 0 && !gate.operational_timeout) Gates in
let Native = O.native_cairn in
let NativePassed = Native.reports_available
  && Native.validation.valid
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
  && std.array.all (fun name => std.array.any (fun check =>
    check.name == name && check.passed
    && (!(std.record.has_field "skipped" check) || !check.skipped)
  ) Native.readiness.checks) ["mcp_agent_smoke", "preserves_corpus_freshness"]
  && GatePassed "native-gates" in
let MeasuredCampaign = fun campaign =>
  campaign.valid_campaign_output && campaign.rounds_completed == 10
  && campaign.command_observation.exit_code == 0
  && !campaign.command_observation.operational_timeout
  && campaign.command_observation.timeout_seconds == 600
  && campaign.maximum_admitted_depth >= 4
  && std.array.all (fun depth => std.array.any (fun capture =>
    capture.child_depth == depth) campaign.overlay_captures) [1, 2, 3, 4]
in
let CampaignsPassed = std.array.all (fun id => std.array.any (fun campaign =>
  campaign.id == id && MeasuredCampaign campaign) Campaigns.campaigns
) ["control23", "planted23"] in
let DeepPassed = Deep.passed && Deep.valid_campaign_output
  && Deep.rounds_completed == 10 && Deep.host_watchdog_timeouts == 0
  # Completed explorer campaigns with retained findings return 1.
  # Valid output and every required snapshot-backed replay remain mandatory.
  && Deep.campaign_command_observation.exit_code == 1
  && !Deep.campaign_command_observation.operational_timeout
  && Deep.campaign_command_observation.timeout_seconds == 600
  && Deep.retained_deep_safety_findings > 0
  && Deep.all_retained_deep_safety_findings_replayed
  && std.array.length Deep.replays == Deep.retained_deep_safety_findings
  && std.array.all (fun replay => replay.passed && replay.parent_depth >= 3
    && replay.verdict.replay_parent_depth == replay.parent_depth
    && replay.verdict.replay_class == "snapshot_backed_reproduced"
    && replay.verdict.reproduced && replay.verdict.snapshot.status == "valid"
    && replay.verdict.snapshot.present && replay.verdict.snapshot.digest_verified
  ) Deep.replays in
let SelectedProbe = Campaigns.selected_planted_probe in
let SelectedVerdict = O.selected_planted_verdict in
let SelectedProbePassed = SelectedProbe.passed && SelectedProbe.summary.accepted
  && SelectedProbe.command_observation.exit_code == 0
  && !SelectedProbe.command_observation.operational_timeout
  && SelectedVerdict.replay_class == "snapshot_backed_reproduced"
  && SelectedVerdict.reproduced && SelectedVerdict.replay_parent_depth > 0
  && SelectedVerdict.snapshot.status == "valid"
  && SelectedVerdict.snapshot.present && SelectedVerdict.snapshot.digest_verified in
let Runtime = O.watchdog in
let WatchdogPassed = Runtime.proof_present && Runtime.passed
  && Runtime.equal && Runtime.fresh_full_equal && Runtime.watchdog_replacement_equal
  && Runtime.safety_invariants_hold
  && Runtime.full_restore_incremental_recapture.memory_equal
  && Runtime.watchdog_recovery.passed
  && Runtime.watchdog_recovery.watchdog_observed
  && Runtime.watchdog_recovery.timer_signals_observed > 0
  && Runtime.watchdog_recovery.poison_enforced
  && Runtime.watchdog_recovery.replacement_created
  && Runtime.watchdog_recovery.restored_parent_memory_equal
  && Runtime.watchdog_recovery.immediate_incremental_recapture.memory_equal
  && Runtime.watchdog_recovery.classification == "operational-host-watchdog-timeout"
  && !Runtime.watchdog_recovery.guest_bug
  && std.array.all (fun field => std.record.get field Runtime.strict_comparison_fields)
    ["initial_memory_identities", "final_memory_identities", "execution_traces",
      "all_admitted_assertion_outcomes", "fault_ledger", "rounds"]
  && std.array.all (fun depth => std.array.any (fun snapshot =>
    snapshot.depth == depth && snapshot.full_reference_memory_equal
    && snapshot.dirty_pages > 0) Runtime.snapshots) [1, 2, 3, 4]
  && GatePassed "watchdog-proof" in
let Matrix = O.kvm_release in
let MatrixPassed = Matrix.receipt_present
  && Matrix.terminal_class == "release-eligible"
  && Matrix.source.revision == O.source.revision && !Matrix.source.dirty
  && std.array.length Matrix.rows == std.array.length MatrixProfile.rows
  && P.unique_by (fun row => row.id) Matrix.rows
  && std.array.all (fun expected => std.array.any (fun actual =>
    actual.id == expected.id && actual.status == "passed") Matrix.rows) MatrixProfile.rows
  && GatePassed "kvm-release" in
let ReleasePassed = CompleteGateInventory && MatrixPassed
  && std.array.all GatePassed ["workspace-tests", "clippy", "formatting", "nix-checks"] in
let EvidenceRetained = M.identity_algorithm == "blake3"
  && M.all_current_manifest_inputs_verified
  && M.all_historical_receipt_bytes_unchanged
  && M.all_historical_required_artifacts_retained
  && Attempts.within_limit in
let IntegrationPassed = O.integration_smokes.all_passed in
let Ready = NativePassed && CampaignsPassed && DeepPassed
  && SelectedProbePassed && WatchdogPassed
  && ReleasePassed && EvidenceRetained && IntegrationPassed in
{
  schema = "chaoscontrol.production-readiness-authorized-extension-receipt.v1",
  owner = "chaoscontrol-vmm",
  maintenance_owner = "chaoscontrol-vmm",
  production_ready | std.contract.Equal Ready = Ready,
  outcome = if Ready then "ready" else "blocked",
  immediate_outcome = O.immediate_outcome,
  durable_capability = "Bounded VM continuation/admission, retained application replay, measured guest shared memory, independent intake-root provenance and executable diagnostic interpreter limits",
  current_consumers = ["explorer branching and replay", "controller replacement", "guest process supervisor", "OCI guest-bundle intake", "SMR adapters", "MVP differential evidence"],
  source = O.source,
  source_archive | Artifact = O.source_archive,
  source_boundary = "The gate commands test exactly this clean implementation revision. A later evidence/documentation-only commit is not represented as that tested revision.",
  implementation_attempts = {
    original_maximum | std.contract.Equal 20 = Attempts.original_maximum_implementation_attempts,
    authorized_maximum | std.contract.Equal 35 = Attempts.maximum_implementation_attempts,
    used | std.contract.Equal 35 = Attempts.used_implementation_attempts,
    within_limit | std.contract.Equal true = Attempts.within_limit,
    authorization = Attempts.authorization,
    counting_rule = Attempts.counting_rule,
    ledger = "dogfood-results/production-readiness-extension-20261017/attempts.ncl",
  },
  required_criteria = [
    { id = "native-cairn-readiness", passed = NativePassed },
    { id = "deep-raft-campaign-and-replay", passed = CampaignsPassed && DeepPassed && SelectedProbePassed },
    { id = "strict-watchdog-continuation", passed = WatchdogPassed },
    { id = "fresh-release-gates", passed = ReleasePassed },
    { id = "reproducible-evidence-retention", passed = EvidenceRetained },
  ],
  native_cairn = O.native_summary,
  upstream_prerequisite = O.upstream_prerequisite,
  campaign_validation = {
    complete_control_and_planted_depth_four = CampaignsPassed,
    campaigns = O.campaign_summary,
    deep_exploration_and_replays = DeepPassed,
    retained_deep_safety_findings = Deep.retained_deep_safety_findings,
    deep_records = "dogfood-results/production-readiness-followup-20261016/deep-probe24-observations.json",
    complete_campaign_records = "dogfood-results/production-readiness-followup-20261016/campaign23-observations.json",
    selected_planted_probe = Campaigns.selected_planted_probe,
    selected_planted_verdict = SelectedVerdict,
    timeout_diagnosis = O.timeout_diagnosis,
    non_claim = "The planted snapshot probe proves the recorded lineage/replay path, not an independently discovered Raft algorithm defect. Safety, liveness, controls and operational timeouts remain distinct.",
  },
  watchdog = O.watchdog_summary,
  release_gates = Gates,
  kvm_release = O.kvm_release_summary,
  integration_smokes = O.integration_smoke_summary,
  observation_records | Artifact = M.observation_records,
  evidence_artifacts | Array Artifact = M.artifacts,
  evidence_retention = O.evidence_retention,
  historical_preservation = O.historical_preservation,
  repeatability = O.repeatability,
  blockers = O.blockers,
  non_claims = O.non_claims,
}
