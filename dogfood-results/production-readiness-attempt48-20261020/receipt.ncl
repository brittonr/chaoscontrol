# Attempt48 verifies one source-policy correction, not production readiness.
let P = import "../../contracts/evidence/profile-primitives.ncl" in
let O = import "observations.json" in
let Attempts = import "attempts.ncl" in
let Artifact = { path | String, digest | P.blake3, bytes | P.u64 } in
let Source = O.source | {
  revision | std.contract.Equal "17b8abdbb294f7fdb4da7e680a05e74c8e0542a0",
  dirty | std.contract.Equal false,
} in
let C = O.commands in
let Passed = fun command => command.exit_code == 0 && !command.operational_timeout in
let PolicyReady = Passed C."focused-policy" in
let CompleteWorkspaceReady = Passed C."workspace-tests-complete"
  && O.workspace_complete.failed == 0 in
let RepeatedWorkspaceReady = CompleteWorkspaceReady && Passed C."workspace-tests"
  && O.workspace.failed == 0 in
let ClippyReady = Passed C.clippy in
let FormattingReady = Passed C.formatting in
let NativeReady = Passed C."native-validation" && Passed C.traceability
  && O.native.valid && O.native.coverage.valid
  && O.native.coverage.verdict == "pass"
  && O.native.coverage.referenced == O.native.coverage.requirements
  && std.array.length O.native.coverage.missing == 0
  && std.array.length O.native.coverage.dangling == 0 in
let NixReady = Passed C."nix-flake-check" in
let ExpectedRows = [
  "build-release-binaries", "build-raft-guest", "deterministic-smp",
  "serialized-snapshot-replay", "virtio-malformed-input", "admitted-drift",
  "fresh-workload-replay",
] in
let KvmReady = Passed C."matrix-execution" && Passed C."matrix-validation-pinned"
  && O.matrix.terminal_class == "release-eligible"
  && std.array.map (fun row => row.id) O.matrix.rows == ExpectedRows
  && std.array.all (fun row => row.status == "passed" && row.exit_code == 0) O.matrix.rows in
let GatesReady = PolicyReady && RepeatedWorkspaceReady && ClippyReady
  && FormattingReady && NativeReady && NixReady && KvmReady in
{
  schema = "chaoscontrol.focused-source-policy-attempt-receipt.v1",
  owner = O.owner,
  source = Source,
  implementation_attempts = {
    used | std.contract.Equal 48 = Attempts.used_implementation_attempts,
    maximum | std.contract.Equal 48 = Attempts.maximum_implementation_attempts,
    within_limit | std.contract.Equal true = Attempts.within_limit,
    authorization = Attempts.authorization,
    counting_rule = Attempts.counting_rule,
  },
  correction_verified | std.contract.Equal PolicyReady = PolicyReady,
  outcome = if GatesReady then "required-gates-passed" else "blocked",
  production_ready | std.contract.Equal false = false,
  policy_controls = O.policy_controls | {
    before | Artifact,
    after | Artifact,
    active_source_denials | std.contract.Equal 4,
    license_denials | std.contract.Equal 2,
    controls_timeout_seconds | std.contract.Equal 120,
    boundary | String,
  },
  workspace_complete_pass | std.contract.Equal CompleteWorkspaceReady = CompleteWorkspaceReady,
  workspace_repeated_pass | std.contract.Equal RepeatedWorkspaceReady = RepeatedWorkspaceReady,
  workspace_initial = O.workspace,
  workspace_complete = O.workspace_complete,
  workspace_failure = O.workspace_failure,
  workspace_boundaries = [O.workspace_boundary, O.complete_workspace_boundary],
  clippy_ready | std.contract.Equal ClippyReady = ClippyReady,
  formatting_ready | std.contract.Equal FormattingReady = FormattingReady,
  native_ready | std.contract.Equal NativeReady = NativeReady,
  native = O.native,
  nix_ready | std.contract.Equal NixReady = NixReady,
  nix_failure = O.nix_failure,
  kvm_release_matrix_ready | std.contract.Equal KvmReady = KvmReady,
  matrix = O.matrix,
  fresh_replay_failure = O.fresh_replay_failure,
  prior_clean_matrix = O.prior_clean_matrix,
  exact_commands = C,
  evidence_artifacts | Array Artifact = O.artifacts,
  historical_receipts | Array Artifact = O.historical_receipts,
  remaining_prerequisites = O.remaining_prerequisites,
  boundary = O.boundary,
}
