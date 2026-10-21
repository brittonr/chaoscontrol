# A validated receipt may honestly compute blocked; export success is not readiness.
let P = import "../../contracts/evidence/profile-primitives.ncl" in
let Prior = import "../production-readiness-attempts49-51-20261020/receipt.ncl" in
let Attempts = import "attempts.ncl" in
let Proof = import "correction-evidence.json" in
let O = import "observations.json" in
let Artifact = { path | String, digest | P.blake3, bytes | P.u64 } in
let Source = O.source | {
  revision | std.contract.Equal "19d2ecd42160eefc2bca0abb2d30c70f490bdea5",
  dirty | std.contract.Equal false,
  all_gate_source_checks_unchanged_and_clean | std.contract.Equal true,
} in
let RequiredCommands = ["clean-workspace-tests", "clean-clippy", "clean-formatting",
  "clean-native-validation", "clean-traceability", "clean-native-readiness",
  "clean-policy-freshness", "clean-nix-flake-check", "clean-kvm-release-matrix",
  "clean-matrix-validation"] in
let CommandsComplete = let names = std.record.fields O.commands in
  std.array.length names == std.array.length RequiredCommands
  && std.array.all (fun name => std.array.elem name names) RequiredCommands in
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
let SmokeReady = std.array.length O.nix_smoke.typed_run_receipts == 2
  && std.array.map (fun run => run.dogfood.selected_workload) O.nix_smoke.typed_run_receipts == ["raft", "rust-workload"]
  && std.array.all (fun run => run.status == "passed" && run.exit_code == 0
    && std.array.length run.static_gates == 13
    && std.array.all (fun gate => gate.status == "pass") run.static_gates
    && run.dogfood.status == "pass" && run.dogfood.expectation_status == "matched"
    && run.dogfood.summary.accepted && run.dogfood.summary.reproduce_exit_status == 0
    && run.dogfood.summary.verdict.reproduced
    && run.dogfood.summary.verdict.replay_class == "snapshot_backed_reproduced"
    && run.dogfood.summary.verdict.snapshot_status == "valid") O.nix_smoke.typed_run_receipts in
let NixReady = Passed "clean-nix-flake-check" && Passed "clean-policy-freshness" && SmokeReady in
let Rows = ["build-release-binaries", "build-raft-guest", "deterministic-smp",
  "serialized-snapshot-replay", "virtio-malformed-input", "admitted-drift", "fresh-workload-replay"] in
let KvmReady = Passed "clean-kvm-release-matrix" && Passed "clean-matrix-validation"
  && O.matrix.source.revision == Source.revision && !O.matrix.source.dirty
  && O.matrix.terminal_class == "release-eligible" && O.matrix.independent_terminal_class == "release-eligible"
  && std.array.map (fun row => row.id) O.matrix.rows == Rows
  && std.array.all (fun row => row.status == "passed" && row.exit_code == 0) O.matrix.rows
  && O.matrix.artifact_verification.all_verified in
let Reproduced = fun v => v.reproduced && v.replay_class == "snapshot_backed_reproduced"
  && v.snapshot.status == "valid" && v.snapshot.present && v.snapshot.digest_verified in
let CorrectionReady = Proof.configuration_unchanged && Proof.kernel_inputs.configuration_byte_identical
  && Proof.run.exit_code == 1 && !Proof.run.operational_timeout
  && Proof.terminal_class == "intended-probe-findings"
  && Proof.assertions.catalog_status == "accepted" && Proof.assertions.collision_safe_evidence
  && std.array.length Proof.replays == 2
  && std.array.all (fun replay => replay.record.exit_code == 0 && !replay.record.operational_timeout
    && Reproduced replay.verdict) Proof.replays in
let W = O.watchdog in
let WatchdogReady = W.passed && W.command.exit_code == 0 && !W.command.operational_timeout
  && W.event.passed && W.event.watchdog_observed && W.event.timer_signal_code == "SI_TIMER"
  && W.event.timer_signals_observed > 0 && W.event.poison_enforced && W.event.replacement_created
  && W.event.restored_parent_memory_equal && W.event.immediate_incremental_recapture.memory_equal
  && W.full_restore_incremental_recapture.memory_equal
  && W.event.classification == "operational-host-watchdog-timeout" && !W.event.guest_bug
  && W.safety_invariants_hold && W.strict_comparisons.initial_memory_identities
  && W.strict_comparisons.final_memory_identities && W.strict_comparisons.execution_traces
  && W.strict_comparisons.all_admitted_assertion_outcomes && W.strict_comparisons.fault_ledger
  && W.strict_comparisons.rounds in
let HistoricalRuntimeReady = Prior.reused_campaign_depth_four_ready && Prior.retained_deep_replays_ready in
let OriginalReady = Prior.selected_original_replay_ready && O.historical_missing_artifact.available_now in
let CurrentEvidenceVerified = O.evidence_verification.all_verified
  && O.evidence_verification.artifacts_checked == std.array.length O.artifacts
  && std.array.length O.evidence_verification.identity_mismatches == 0
  && O.historical_preservation.all_unchanged && Attempts.within_limit
  && std.array.all (fun input => input.byte_identity_verified) O.retained_inputs in
let RequiredEvidenceReady = CurrentEvidenceVerified && O.historical_missing_artifact.available_now
  && std.array.length O.historical_inaccessible_nix_payloads == 0 in
let LocalGatesReady = CommandsComplete && CargoReady && TraceabilityReady && NixReady && KvmReady in
let AllGatesReady = LocalGatesReady && NativeReady in
let Ready = AllGatesReady && CorrectionReady && WatchdogReady && HistoricalRuntimeReady
  && OriginalReady && RequiredEvidenceReady in
{
  schema = "chaoscontrol.release-correction52-receipt.v1",
  owner = "chaoscontrol-vmm",
  maintenance_owner = O.maintenance_owner,
  immediate_outcome = O.immediate_outcome,
  durable_capability = "Offset-only built-in ftrace record validation through the common guest-kernel builder; unchanged full KCOV/ftrace and weak-record admission",
  current_consumer = "The existing bounded Rust-workload and Raft KVM/replay rails",
  source = Source,
  implementation_attempts = {
    original_maximum | std.contract.Equal 20 = Attempts.original_maximum_implementation_attempts,
    authorized_maximum | std.contract.Equal 52 = Attempts.maximum_implementation_attempts,
    used | std.contract.Equal 52 = Attempts.used_implementation_attempts,
    within_limit | std.contract.Equal true = Attempts.within_limit,
    authorization = Attempts.authorization,
    attempts = Attempts.attempts,
  },
  original_acceptance_criteria_unchanged | std.contract.Equal true = true,
  complete_gate_inventory | std.contract.Equal true = CommandsComplete,
  bootstrap_correction_verified | std.contract.Equal true = CorrectionReady,
  cargo_ready = CargoReady,
  native_traceability_ready = TraceabilityReady,
  native_readiness_ready = NativeReady,
  nix_ready = NixReady,
  full_nix_vm_smoke_ready = SmokeReady,
  kvm_release_matrix_ready = KvmReady,
  local_release_gates_ready = LocalGatesReady,
  required_release_gates_ready = AllGatesReady,
  strict_watchdog_ready | std.contract.Equal true = WatchdogReady,
  reused_campaign_depth_four_ready = Prior.reused_campaign_depth_four_ready,
  retained_deep_replays_ready = Prior.retained_deep_replays_ready,
  selected_original_replay_ready = OriginalReady,
  current_evidence_verified | std.contract.Equal true = CurrentEvidenceVerified,
  required_evidence_retained = RequiredEvidenceReady,
  production_ready | std.contract.Equal Ready = Ready,
  outcome = if Ready then "ready" else "blocked",
  exact_commands = O.commands,
  workspace = O.workspace,
  native = N,
  matrix = O.matrix,
  nix_smoke = O.nix_smoke,
  correction = Proof,
  watchdog = W,
  new_finding_inventory = O.new_finding_inventory,
  new_retained_deep_findings = O.new_retained_deep_findings,
  reused_runtime = Prior.reused_runtime,
  reused_runtime_source_boundary = O.reused_runtime_source_boundary,
  historical_missing_artifact = O.historical_missing_artifact,
  historical_inaccessible_nix_payloads = O.historical_inaccessible_nix_payloads,
  historical_preservation = O.historical_preservation,
  local_input_retention = O.retained_inputs,
  evidence_verification = O.evidence_verification,
  evidence_artifacts | Array Artifact = O.artifacts,
  remaining_prerequisites = O.remaining_prerequisites,
  non_claims = O.non_claims,
}
