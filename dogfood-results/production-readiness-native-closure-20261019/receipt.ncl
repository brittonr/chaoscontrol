# Native traceability closure and release-gate receipt.
let P = import "../../contracts/evidence/profile-primitives.ncl" in
let O = import "observations.json" in
let Artifact = { path | String, digest | P.blake3, bytes | P.u64 } in
let NativeReady = O.native_traceability.valid == true
  && O.native_traceability.verdict == "pass"
  && std.array.length O.native_traceability.missing == 0
  && std.array.length O.native_traceability.dangling == 0
  && O.native_traceability.referenced == O.native_traceability.requirements in
let CargoReady = O.cargo.workspace.failed == 0
  && O.cargo.clippy_exit == 0
  && O.cargo.formatting_exit == 0
  && O.evidence_contracts.exit == 0 in
let WatchdogReady = O.watchdog.proof_present == true
  && O.watchdog.passed == true
  && O.watchdog.exit_code == 0
  && O.watchdog.operational_timeout == false in
let NixReady = O.nix_flake_check.executed == true
  && O.nix_flake_check.exit_code == 0
  && O.nix_flake_check.operational_timeout == false in
let KvmReady = if O.kvm_release_matrix.executed == true
  && O.kvm_release_matrix.exit_code == 0
  && O.kvm_release_matrix.operational_timeout == false
  && O.kvm_release_matrix.receipt != null
  then O.kvm_release_matrix.receipt.value.valid == true
  else false in
let CutoverReady = O.campaign_cutover.source_revision == O.implementation_revision
  && O.campaign_cutover.decision_equal == true
  && std.array.all (fun count => count == 10) O.campaign_cutover.parity_rounds in
let McpReady = false in
let Ready = NativeReady && CargoReady && WatchdogReady && NixReady && KvmReady
  && CutoverReady && McpReady in
{
  schema = "chaoscontrol.production-readiness-native-closure-receipt.v1",
  owner = "chaoscontrol-vmm",
  implementation_revision = O.implementation_revision,
  current_source_revision = O.current_source_revision,
  evidence_revision = O.evidence_revision,
  production_ready | std.contract.Equal Ready = Ready,
  outcome = if Ready then "ready" else "blocked",
  native_traceability_ready = NativeReady,
  native_traceability = O.native_traceability,
  cargo_ready = CargoReady,
  cargo = O.cargo,
  evidence_contracts = O.evidence_contracts,
  nickel_cohort_nix = O.nickel_cohort_nix,
  campaign_cutover_ready = CutoverReady,
  campaign_cutover = O.campaign_cutover,
  watchdog_ready = WatchdogReady,
  rust_workload_jiffies_diagnostic = O.rust_workload_jiffies_diagnostic,
  watchdog = O.watchdog,
  nix_flake_check_ready = NixReady,
  nix_flake_check = O.nix_flake_check,
  kvm_release_matrix_ready = KvmReady,
  kvm_release_matrix = O.kvm_release_matrix,
  mcp_agent_smoke_ready = McpReady,
  cairn_release_readiness = O.cairn_release_readiness,
  evidence_artifacts | Array Artifact = O.artifacts,
  blockers = std.array.filter (fun value => value != null) [
    if NativeReady then null else "native traceability is not 382/382 with zero missing and zero dangling",
    if CargoReady then null else "Cargo formatting, Clippy, tests, or evidence contracts did not pass",
    if CutoverReady then null else "Campaign cutover receipt or frontier parity is not admissible",
    if WatchdogReady then null else "fresh strict watchdog proof did not complete",
    if NixReady then null else "required Nix flake check did not pass",
    if KvmReady then null else "seven-row KVM release matrix did not pass",
    if McpReady then null else O.cairn_release_readiness.mcp_agent_smoke,
  ],
  non_claims = O.non_claims,
}
