# Consumer-owned selection, activated only with the verified compatible cohort.
# Import this record into default.ncl's release_readiness_policy after migration.
let Registry = import "../fixtures/agent-completion/registry.ncl" in
{
  id = "chaoscontrol-required-checks-v1",
  mcp_completion_contract = "fixtures/agent-completion/contract.json",
  mcp_completion_results = "target/cairn-completion/results.json",
  preserves_corpus = "fixtures/preserves-canonicalization/corpus.json",
  required_external_commands = Registry.required_external_commands,
}
