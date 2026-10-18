# Passing command status is never a substitute for the required report verdict.
let P = import "../../contracts/evidence/profile-primitives.ncl" in
let O = import "observations.json" in
let M = import "evidence-manifest.json" in
let Attempts = import "attempts.ncl" in
let MatrixProfile = import "../../contracts/kvm-release/matrix.json" in
let Artifact = { path | String, digest | P.blake3, bytes | P.u64 } in
let AttemptObservation = {
  number | P.integer_between 41 45,
  correction | String,
  status | std.contract.from_predicate (fun value =>
    std.array.elem value ["passed", "failed", "blocked"]),
  source_revision | String,
  evidence | Array Artifact,
  non_claim | String,
} in
let ObservedAttempts = O.attempts | Array AttemptObservation in
let CompleteAttemptInventory = std.array.length ObservedAttempts == 5
  && P.unique_by (fun attempt => attempt.number) ObservedAttempts
  && std.array.all (fun number => std.array.any (fun attempt =>
    attempt.number == number) ObservedAttempts) [41, 42, 43, 44, 45] in
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
  && campaign.capture_alignment_verified
  # Completed finding-producing campaigns return 1; admitted completion is still mandatory.
  && std.array.elem campaign.command_observation.exit_code [0, 1]
  && !campaign.command_observation.operational_timeout
  && campaign.command_observation.timeout_seconds == 600
  && campaign.maximum_admitted_depth >= 4
  && std.array.all (fun depth => std.array.any (fun capture =>
    capture.child_depth == depth) campaign.overlay_captures) [1, 2, 3, 4]
in
let CampaignsPassed = std.array.all (fun id => std.array.any (fun campaign =>
  campaign.id == id && MeasuredCampaign campaign) O.campaigns
) ["control", "planted"] in
let Reproduced = fun replay => replay.command_observation.exit_code == 0
  && !replay.command_observation.operational_timeout
  && replay.passed && replay.parent_depth >= 3
  && replay.verdict.replay_parent_depth == replay.parent_depth
  && replay.verdict.replay_class == "snapshot_backed_reproduced"
  && replay.verdict.reproduced && replay.verdict.snapshot.status == "valid"
  && replay.verdict.snapshot.present && replay.verdict.snapshot.digest_verified in
let DeepPassed = O.deep_replay_inventory_complete
  && O.fresh_deep_campaign_passed
  && std.array.length O.deep_replays > 0
  && std.array.all Reproduced O.deep_replays in
let Selected = O.selected_planted_verdict in
let SelectedPassed = Selected.replay_class == "snapshot_backed_reproduced"
  && Selected.reproduced && Selected.replay_parent_depth > 0
  && Selected.snapshot.status == "valid"
  && Selected.snapshot.present && Selected.snapshot.digest_verified in
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
    && snapshot.dirty_pages > 0 && snapshot.overlay_payload_bytes > 0
  ) Runtime.snapshots) [1, 2, 3, 4]
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
let ReleasePassed = CompleteGateInventory && !O.source.dirty && MatrixPassed
  && std.array.all GatePassed ["workspace-tests", "clippy", "formatting", "nix-checks"] in
let EvidenceRetained = M.identity_algorithm == "blake3"
  && M.all_current_manifest_inputs_verified
  && M.all_historical_receipt_bytes_unchanged
  && M.all_historical_required_artifacts_retained
  && Attempts.within_limit in
let Ready = NativePassed && CampaignsPassed && DeepPassed && SelectedPassed
  && WatchdogPassed && ReleasePassed && EvidenceRetained in
{
  schema = "chaoscontrol.production-readiness-attempts41-45-receipt.v1",
  owner = "chaoscontrol-vmm",
  maintenance_owner = "chaoscontrol-vmm",
  production_ready | std.contract.Equal Ready = Ready,
  outcome = if Ready then "ready" else "blocked",
  complete_gate_inventory | std.contract.Equal true = CompleteGateInventory,
  immediate_outcome = O.immediate_outcome,
  durable_capability = "Per-VM frontier marker ownership and capacity, facts-only architectural admission, actual CPU/complete-VM continuation, pure virtio decisions with adversarial consumers, and exact-cohort report-based readiness evidence",
  current_consumers = ["Raft SMR consumer", "explorer branching and replay", "controller replacement", "release operator"],
  source = O.source,
  source_archive | Artifact = O.source_archive,
  prior_source = O.prior_source,
  source_boundary = "Each cohort retains its exact source and commands. The corrected clean implementation is not the prior cohort, and a later evidence-only delivery commit is not release-tested by these commands.",
  implementation_attempts = {
    original_maximum | std.contract.Equal 20 = Attempts.original_maximum_implementation_attempts,
    authorized_maximum | std.contract.Equal 45 = Attempts.maximum_implementation_attempts,
    used | std.contract.Equal 45 = Attempts.used_implementation_attempts,
    within_limit | std.contract.Equal true = Attempts.within_limit,
    authorization = Attempts.authorization,
    counting_rule = Attempts.counting_rule,
    historical_ledger = Attempts.previous_ledger,
    complete_observation_inventory | std.contract.Equal true = CompleteAttemptInventory,
    observed_attempts = ObservedAttempts,
  },
  required_criteria = [
    { id = "native-cairn-readiness", passed = NativePassed },
    { id = "deep-raft-campaign-and-replay", passed = CampaignsPassed && DeepPassed && SelectedPassed },
    { id = "strict-watchdog-continuation", passed = WatchdogPassed },
    { id = "fresh-release-gates", passed = ReleasePassed },
    { id = "reproducible-evidence-retention", passed = EvidenceRetained },
  ],
  native_cairn = O.native_summary,
  delivery_metadata_check = O.delivery_metadata_check,
  upstream_prerequisite = O.upstream_prerequisite,
  campaign_validation = {
    complete_control_and_planted_depth_four = CampaignsPassed,
    campaigns = O.campaign_summary,
    deep_exploration_and_all_retained_replays = DeepPassed,
    deep_replay_inventory_complete = O.deep_replay_inventory_complete,
    retained_deep_replays = std.array.map (fun replay => {
      id = replay.id,
      parent_depth = replay.parent_depth,
      bug = replay.bug,
      verdict_artifact = replay.verdict_artifact,
      passed = replay.passed,
      replay_class = if replay.verdict == null then null else replay.verdict.replay_class,
    }) O.deep_replays,
    fresh_export_resumption = {
      plan = O.fresh_deep_replay_inventory.resumption_plan,
      initial_failure_diagnostic = O.fresh_deep_replay_inventory.initial_failure_diagnostic,
      boundary = O.fresh_deep_replay_inventory.boundary,
    },
    selected_original_planted_verdict = Selected,
    selected_current_planted = O.selected_current_planted,
    explicit_original_engine_replays = O.legacy_cohort_replays,
    timeout_diagnosis = O.timeout_diagnosis,
    non_claim = "Old incomplete snapshots remain rejected. A fresh selected probe cannot repair or replace an older retained finding. Safety, liveness, deliberate probes and operational timeouts remain separate.",
  },
  consumer_controls = O.integration_smoke_summary,
  watchdog = O.watchdog_summary,
  release_gates = Gates,
  kvm_release = O.kvm_release_summary,
  prior_cohort = O.prior_cohort,
  observation_records | Artifact = M.observation_records,
  evidence_artifacts | Array Artifact = M.artifacts,
  evidence_retention = O.evidence_retention,
  historical_preservation = O.historical_preservation,
  repeatability = O.repeatability,
  blockers = O.blockers,
  non_claims = O.non_claims,
}
