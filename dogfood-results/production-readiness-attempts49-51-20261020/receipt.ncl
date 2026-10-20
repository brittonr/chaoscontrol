# Scoped corrections, independently checked gates, and unchanged original blockers.
let P = import "../../contracts/evidence/profile-primitives.ncl" in
let O = import "observations.json" in
let Attempts = import "attempts.ncl" in
let Artifact = { path | String, digest | P.blake3, bytes | P.u64 } in
let Source = O.source | {
  revision | std.contract.Equal "ab006a79ce6de78ca40cbdde60d35efce07513a9",
  dirty | std.contract.Equal false,
  ..
} in
let RequiredCommands = ["clean-workspace-tests", "clean-clippy", "clean-formatting",
  "clean-native-validation", "clean-traceability", "clean-native-readiness",
  "clean-policy-freshness", "clean-nix-flake-check", "clean-kvm-release-matrix",
  "clean-matrix-validation"] in
let CommandsComplete = std.array.length (std.record.fields O.commands) == 10
  && std.array.all (fun name => std.array.elem name (std.record.fields O.commands)) RequiredCommands in
let Passed = fun name => let c = std.record.get name O.commands in
  c.executed && c.source_revision == Source.revision && c.exit_code == 0 && !c.operational_timeout in
let CargoReady = std.array.all Passed ["clean-workspace-tests", "clean-clippy", "clean-formatting"]
  && O.workspace.failed == 0 && O.workspace.all_reported_results_passed in
let N = O.native in
let TraceabilityReady = Passed "clean-native-validation" && Passed "clean-traceability"
  && N.validation.valid && N.coverage.valid && N.coverage.verdict == "pass"
  && N.coverage.referenced == N.coverage.requirements
  && std.array.length N.coverage.missing == 0 && std.array.length N.coverage.dangling == 0 in
let NativeReady = TraceabilityReady && Passed "clean-native-readiness"
  && N.readiness.valid && N.readiness.verdict == "pass" && N.readiness.onix_release_ready
  && std.array.all (fun check => check.passed) N.readiness.checks in
let NixReady = Passed "clean-nix-flake-check" && Passed "clean-policy-freshness" in
let Rows = ["build-release-binaries", "build-raft-guest", "deterministic-smp",
  "serialized-snapshot-replay", "virtio-malformed-input", "admitted-drift", "fresh-workload-replay"] in
let KvmReady = Passed "clean-kvm-release-matrix" && Passed "clean-matrix-validation"
  && O.matrix.source.revision == Source.revision && !O.matrix.source.dirty
  && O.matrix.terminal_class == "release-eligible"
  && O.matrix.independent_terminal_class == "release-eligible"
  && std.array.map (fun row => row.id) O.matrix.rows == Rows
  && std.array.all (fun row => row.status == "passed" && row.exit_code == 0) O.matrix.rows
  && O.matrix.all_artifacts_verified in
let W = O.corrections.watchdog in
let WatchdogReady = W.passed && W.event.passed && W.event.watchdog_observed
  && W.event.timer_signal_code == "SI_TIMER" && W.event.timer_signals_observed > 0
  && W.event.poison_enforced && W.event.replacement_created
  && W.event.restored_parent_memory_equal && W.event.immediate_incremental_recapture.memory_equal
  && W.event.classification == "operational-host-watchdog-timeout" && !W.event.guest_bug
  && W.strict_comparisons.initial_memory_identities && W.strict_comparisons.final_memory_identities
  && W.strict_comparisons.execution_traces && W.strict_comparisons.all_admitted_assertion_outcomes
  && W.strict_comparisons.fault_ledger && W.strict_comparisons.rounds in
let Reproduced = fun probe => probe.accepted && probe.replay_class == "snapshot_backed_reproduced" in
let CorrectionsVerified = O.corrections.test_inventory.failed == 0
  && Reproduced O.corrections.fresh_probe && O.corrections.controlled_probe.accepted
  && O.corrections.controlled_probe.supervisor_byte_identity.identical && WatchdogReady in
let R = O.reused_runtime in
let CampaignsReady = R.campaigns.complete_control_and_planted_depth_four
  && R.campaigns.completed_control_and_planted_under_original_bounds
  && std.array.all (fun id => std.array.any (fun campaign => campaign.id == id
    && campaign.rounds == 10 && campaign.branches == 20 && campaign.maximum_admitted_depth >= 4
    && campaign.valid_campaign_output && campaign.capture_alignment_verified
    && !campaign.operational_timeout && std.array.all (fun depth => std.array.any
      (fun capture => capture.child_depth == depth && capture.overlay_payload_bytes > 0)
      campaign.overlay_captures) [1, 2, 3, 4]) R.campaigns.observations) ["control", "planted"] in
let DeepReady = R.deep_replays.all_current_retained_deep_safety_findings_replayed
  && std.array.length R.deep_replays.observations == 8
  && std.array.all (fun replay => replay.passed && replay.parent_depth >= 3) R.deep_replays.observations in
let OriginalReady = R.deep_replays.selected_original_current_source_attempt.executed
  && R.deep_replays.selected_original_current_source_attempt.passed in
let CurrentEvidenceVerified = O.evidence_verification.all_verified
  && O.historical_preservation.all_unchanged && Attempts.within_limit in
let EvidenceReady = CurrentEvidenceVerified
  && O.historical_missing_artifact.available_now
  && std.array.length O.nix_failure.inaccessible_artifacts == 0 in
let GatesReady = CommandsComplete && CargoReady && NativeReady && NixReady && KvmReady in
let Ready = CorrectionsVerified && GatesReady && CampaignsReady && DeepReady
  && OriginalReady && WatchdogReady && EvidenceReady in
{
  schema = "chaoscontrol.release-corrections49-51-receipt.v1",
  owner = "chaoscontrol-vmm",
  maintenance_owner = "ChaosControl VMM/explorer maintainers; upstream producer admission: Cairn CLI/core maintainers",
  immediate_outcome = O.immediate_outcome,
  durable_capability = "Isolated process-global signal tests, compiler-preserving Cargo selection, and shared immutable replay parents with per-parent/depth persistence",
  current_consumer = "ChaosControl explorer, Octet consumer checks, and bounded snapshot-backed replay launcher",
  adoption_path = "Existing first-party runtime and release paths; unchanged snapshot encoding, assertion admission and declared bounds",
  source = Source,
  proof_source_boundary = O.corrections.source_context,
  implementation_attempts = {
    original_maximum | std.contract.Equal 20 = Attempts.original_maximum_implementation_attempts,
    authorized_maximum | std.contract.Equal 51 = Attempts.maximum_implementation_attempts,
    used | std.contract.Equal 51 = Attempts.used_implementation_attempts,
    within_limit | std.contract.Equal true = Attempts.within_limit,
    authorization = Attempts.authorization,
    counting_rule = Attempts.counting_rule,
    attempts = Attempts.attempts,
  },
  original_acceptance_criteria_unchanged | std.contract.Equal true = true,
  complete_gate_inventory | std.contract.Equal true = CommandsComplete,
  corrections_verified | std.contract.Equal CorrectionsVerified = CorrectionsVerified,
  cargo_ready = CargoReady,
  native_traceability_ready = TraceabilityReady,
  native_readiness_ready = NativeReady,
  nix_ready = NixReady,
  kvm_release_matrix_ready = KvmReady,
  strict_watchdog_ready = WatchdogReady,
  reused_campaign_depth_four_ready = CampaignsReady,
  retained_deep_replays_ready = DeepReady,
  selected_original_replay_ready = OriginalReady,
  required_release_gates_ready = GatesReady,
  current_evidence_verified = CurrentEvidenceVerified,
  required_evidence_retained = EvidenceReady,
  original_historical_input_available = O.historical_missing_artifact.available_now,
  production_ready | std.contract.Equal Ready = Ready,
  outcome = if Ready then "ready" else "blocked",
  exact_commands = O.commands,
  workspace = O.workspace,
  native = N,
  matrix = O.matrix,
  corrections = O.corrections,
  reused_runtime = R,
  prior_failed_nix_run = O.first_release_run,
  nix_format_equivalence = O.nix_format_equivalence,
  nix_failure = O.nix_failure,
  octet_reports = O.octet_reports,
  historical_preservation = O.historical_preservation,
  historical_missing_artifact = O.historical_missing_artifact,
  evidence_verification = O.evidence_verification,
  evidence_artifacts | Array Artifact = O.artifacts,
  remaining_prerequisites = O.remaining_prerequisites,
  non_claims = O.non_claims,
}
