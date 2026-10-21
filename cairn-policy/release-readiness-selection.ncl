# Consumer-owned selection for the verified compatible cohort.
# default.ncl activates this record through the typed release_readiness_policy.
let Registry = import "../fixtures/agent-completion/registry.ncl" in
{
  id = "chaoscontrol-required-checks-v1",
  mcp_completion_contract = "target/cairn-completion-inputs/contract.json",
  mcp_completion_results = "target/cairn-completion/results.json",
  preserves_corpus = "fixtures/preserves-canonicalization/corpus.json",
  required_external_commands = Registry.required_external_commands,
}
