# Human-authored closure receipt over retained Rust/runtime and command observations.
# This record cannot turn successful process execution into passing native reports.
let P = import "../../contracts/evidence/profile-primitives.ncl" in
let Observed = import "release-observations.json" in
let Attempts = import "attempts.ncl" in
let Campaigns = import "campaign-observations.json" in
let Runtime = import "watchdog-current-artifacts-observations.json" in
let Upstream = import "cairn-current-upstream-audit.json" in
let Preservation = import "historical-preservation.json" in
let Artifact = {
  path | String,
  digest | P.blake3,
  bytes | P.u64,
} in
let Gate = {
  id | String,
  passed | Bool,
  exit_code | Number,
  operational_timeout | Bool,
  report_passed | Bool,
  observation | Artifact,
} in
let Native = Observed.native_cairn in
let TraceabilityPassed = Native.traceability.valid
  && Native.traceability.verdict == "pass"
  && std.array.length Native.traceability.missing == 0
  && std.array.length Native.traceability.dangling == 0
  && Native.traceability.referenced == Native.traceability.requirements in
let NativePassed = Native.validation.valid
  && std.array.length Native.validation.issues == 0
  && std.array.length Native.validation.findings == 0
  && TraceabilityPassed
  && Native.readiness.valid
  && Native.readiness.verdict == "pass"
  && Native.readiness.coverage_audit_valid
  && std.array.length Native.readiness.checks > 0
  && std.array.all (fun check => check.passed) Native.readiness.checks
  && Native.summary.all_required_passed in
let MeasuredDepthFour = fun campaign =>
  campaign.valid_campaign_output
  && campaign.maximum_admitted_depth >= 4
  && std.array.all (fun depth =>
    std.array.any (fun capture => capture.child_depth == depth)
      campaign.overlay_captures
  ) [1, 2, 3, 4]
in
let ControlMeasured = std.array.any (fun campaign =>
  std.string.is_match "^control" campaign.id && MeasuredDepthFour campaign
) Campaigns.campaigns in
let PlantedMeasured = std.array.any (fun campaign =>
  std.string.is_match "^planted" campaign.id && MeasuredDepthFour campaign
) Campaigns.campaigns in
let CampaignPassed = ControlMeasured && PlantedMeasured
  && Campaigns.selected_planted_probe.passed
  && Observed.deep_exploration_replay_satisfied in
let Gates = Observed.gates | Array Gate in
let ReleaseGates = std.array.filter (fun gate => std.array.elem gate.id
  ["clean-workspace-tests", "clean-clippy", "clean-formatting", "clean-nix-checks", "clean-kvm-release"]
) Gates in
let GatesPassed = std.array.length Gates == 7
  && P.unique_by (fun gate => gate.id) Gates
  && std.array.length ReleaseGates == 5
  && std.array.all (fun gate =>
    gate.passed && gate.exit_code == 0
      && !gate.operational_timeout && gate.report_passed
  ) ReleaseGates in
let MatrixPassed = Observed.kvm_release.receipt_present
  && Observed.kvm_release.terminal_class == "release-eligible"
  && Observed.kvm_release.source.revision == Observed.source.revision
  && !Observed.kvm_release.source.dirty
  && std.array.length Observed.kvm_release.rows == 7
  && std.array.all (fun row => row.status == "passed") Observed.kvm_release.rows in
let EvidenceRetained = Preservation.all_historical_files_unchanged
  && Attempts.within_limit
  && Observed.evidence_retention.identity_algorithm == "blake3"
  && Observed.evidence_retention.all_manifest_inputs_observed_as_files
  && Observed.evidence_retention.all_required_artifacts_retained in
let Ready = NativePassed && CampaignPassed
  && Observed.watchdog.passed && GatesPassed && MatrixPassed && EvidenceRetained in
{
  schema = "chaoscontrol.production-readiness-closure-receipt.v1",
  owner = "chaoscontrol-vmm",
  production_ready | Bool = Ready,
  outcome = if Ready then "ready" else "blocked",
  immediate_outcome = Observed.immediate_outcome,
  durable_capability = "Regression-backed single-vCPU exit and timer handling, an actual watchdog replacement proof, and exact-source gate/evidence accounting without weakening strict memory comparison",
  current_consumers = ["explorer frontier branching", "retained-parent restore", "production controller replacement"],
  maintenance_owner = "chaoscontrol-vmm",
  source = Observed.source,
  source_archive | Artifact = Observed.source_archive,
  evidence_only_followup_boundary = "Release commands tested the exact clean revision above. The receipt and documentation delivery does not claim that a later evidence-only commit was itself the tested source revision.",
  implementation_attempts = {
    maximum | std.contract.Equal 20 = Attempts.maximum_implementation_attempts,
    used | P.integer_between 0 20 = Attempts.used_implementation_attempts,
    within_limit | std.contract.Equal true = Attempts.within_limit,
    ledger = "dogfood-results/production-readiness-closure-20261015/attempts.ncl",
    counting_rule = Attempts.counting_rule,
    failed_kernel_disposition = Attempts.failed_kernel_disposition,
  },
  required_criteria = [
    { id = "native-cairn-readiness", passed = NativePassed },
    { id = "deep-raft-campaign-and-replay", passed = CampaignPassed },
    { id = "strict-watchdog-continuation", passed = Observed.watchdog.passed },
    { id = "fresh-release-gates", passed = GatesPassed && MatrixPassed },
    { id = "reproducible-evidence-retention", passed = EvidenceRetained },
  ],
  native_cairn = Native,
  latest_upstream = {
    revision = Upstream.upstream_revision,
    source_archive | Artifact = Upstream.source_archive,
    readiness_module = Upstream.readiness_module,
    source_access_is_remaining_blocker = false,
    upstream_prerequisite = Upstream.upstream_prerequisite,
    update_applied = Upstream.upstream_update_applied,
  },
  campaign_validation = {
    complete_control_with_depth_four_overlays = ControlMeasured,
    complete_planted_with_depth_four_overlays = PlantedMeasured,
    deep_exploration_replay_satisfied = Observed.deep_exploration_replay_satisfied,
    selected_planted_probe = Observed.selected_planted_probe,
    campaign_inventory = Observed.campaign_inventory,
    exact_inputs = "dogfood-results/production-readiness-closure-20261015/raft-campaigns.ncl",
    complete_records = "dogfood-results/production-readiness-closure-20261015/campaign-observations.json",
    timeout_diagnosis = Observed.timeout_diagnosis,
    boundary = "Completed rounds, an empty finding set, or a rejected report do not satisfy the native requirement for an accepted deep exploration replay. Historical deep findings remain diagnostic-only.",
  },
  watchdog = Observed.watchdog,
  frozen_source_three_controller_comparison = {
    passed = Runtime.passed,
    exact_profile = Runtime.profile_inputs,
    snapshots = Runtime.snapshots,
    watchdog_event = Runtime.watchdog_recovery,
    strict_comparison_fields = Runtime.strict_comparison_fields,
    assertion_coverage = std.array.map (fun replay => {
      name = replay.name,
      coverage = replay.assertion_coverage,
      outcomes_identity = replay.all_admitted_assertion_outcomes_digest,
      execution_identities = replay.execution_traces,
      initial_memory_identities = replay.initial_memory_identities,
      final_memory_identities = replay.final_memory_identities,
      round_observations_identity = replay.round_observations_digest,
      fault_observed = replay.fault_observed,
    }) Runtime.replays,
    complete_records = "dogfood-results/production-readiness-closure-20261015/watchdog-current-artifacts-observations.json",
    boundary = "The actual watchdog event, replacement, parent fidelity and recapture passed. Strict final memory did not. Every admitted assertion outcome was compared; unexercised outcomes are not coverage.",
  },
  release_gates = Gates,
  kvm_release = Observed.kvm_release,
  structural_native_gates = Observed.structural_native_gates,
  evidence_artifacts | Array Artifact = Observed.evidence_artifacts,
  historical_files_preserved | std.contract.Equal true = Preservation.all_historical_files_unchanged,
  historical_preservation_manifest = "dogfood-results/production-readiness-closure-20261015/historical-preservation.json",
  local_retention = Observed.local_retention,
  repeatability = Observed.repeatability,
  blockers = Observed.blockers,
  non_claims = Observed.non_claims,
}
