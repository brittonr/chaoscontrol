let Prior = import "../production-readiness-attempt52-20261020/attempts.ncl" in
{
  schema = "chaoscontrol.production-readiness-mcp53.v1",
  owner = "chaoscontrol-vmm",
  previous_used_attempts | std.contract.Equal 52 = Prior.used_implementation_attempts,
  maximum_implementation_attempts | std.contract.Equal 53 = Prior.maximum_implementation_attempts + 1,
  authorization = {
    direct_request = "fix cairn mcp",
    interpretation = "One new MCP-specific correction, including the Cairn-owned producer/consumer mechanism and supported ChaosControl integration; not an unrestricted continuation",
    additional_attempts | std.contract.Equal 1 = 1,
    preserved_boundaries = [
      "No acceptance criterion, check, required command or dependency is suppressed or replaced by copied fixtures",
      "No historical replay finding or missing artifact is substituted or promoted",
      "No host reconfiguration, manual credential access, deployment or unrelated sibling edits",
      "Existing producer profile bounds remain unchanged; new closed consumer kinds retain the consumer's already-declared command deadlines and explicitly bounded streams",
      "No attempt54 is implied",
    ],
  },
  attempts = [{
    number = 53,
    correction = "Fix Cairn downstream MCP input selection and exact policy propagation; reuse bounded dependency/native Nix support and admit the actual full portable consumer commands",
    status = 'verified,
    source_plan = "Cairn .cairn/changes/fix-downstream-mcp-readiness and ChaosControl .cairn/changes/refresh-vendored-cairn-policy",
    evidence_boundary = "The immutable producer cohort, policy migration, full consumer completion, original CLI/MCP verdicts, native readiness and independent KVM matrix bind this correction; unchanged historical and inaccessible-payload blockers remain",
  }],
  used_implementation_attempts | std.contract.Equal 53 = previous_used_attempts + std.array.length attempts,
  within_limit | std.contract.Equal true = used_implementation_attempts <= maximum_implementation_attempts,
  execution_attempts = {
    note = "Operational repetitions of the same attempt are not new corrective attempt numbers; failed and unsupported observations remain retained and nonpassing.",
    retained_nonpassing = [
      "Cohort 4d98c4b consumer completion: Cargo linking failed after the measured environment dropped Nix host-role selectors",
      "Cohort 03c513c/consumer runs: global and checkout temporary filesystems exhausted; command recorder and three gates failed on quota",
      "Consumer completion at 9ad22fb: Nix smoke produced HostWatchdogTimeout for both probes; classified operational, not guest evidence",
      "Consumer completion at be2ef3a: Nix smoke produced HostWatchdogTimeout for the rust-workload probe only; raft passed with snapshot-backed reproduction",
    ],
    passing_repetitions_are_independently_identified = true,
    result_hashes_rebound = false,
  },
}
