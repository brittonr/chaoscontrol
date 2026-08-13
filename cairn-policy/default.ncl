let Upstream = import "upstream-default.ncl" in
let ChaosControlTraceabilityProfile = {
  id = "chaoscontrol-default",
  requirement_sources = [
    { root = ".cairn/specs", source_kind = "accepted_specs", extensions = ["md"] },
  ],
  evidence_sources = [
    { root = "crates", source_kind = "rust_source", extensions = ["rs"] },
    { root = "scripts", source_kind = "tool_source", extensions = ["rs"] },
    { root = "tools", source_kind = "tool_source", extensions = ["rs"] },
    { root = "cairn-policy", source_kind = "nickel_source", extensions = ["ncl"] },
  ],
  marker_verbs = ["impl", "verify", "depends", "related"],
} in
Upstream & {
  name | force = "chaoscontrol-default",
  policy_schema_compatibility | force = Upstream.policy_schema_compatibility & {
    refresh_command | force = "nix run path:$CAIRN_SOURCE#cairn -- policy export --source cairn-policy/default.ncl --output cairn-policy/generated/cairn-policy.json",
  },
  traceability_policy | force = {
    default_profile = "chaoscontrol-default",
    profiles = [
      ChaosControlTraceabilityProfile,
      ChaosControlTraceabilityProfile & { id | force = "cairn-default" },
    ],
  },
}
