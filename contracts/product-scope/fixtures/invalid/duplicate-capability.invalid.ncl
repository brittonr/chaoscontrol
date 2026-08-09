let ProductScope = import "../../product-scope.ncl" in
let Capability = {
  id = "duplicate-capability",
  name = "Duplicate capability",
  owner = "chaoscontrol-evidence",
  state = "experimental",
  status_label = "experimental",
  evidence = { id = "missing-evidence", state = "missing", source = "missing.json" },
  boundary = "Invalid fixture.",
  documentation_targets = ["README.md"],
  non_claims = ["No promotion."],
} in
{
  schema_version = 1,
  authority = {
    workspace_manifest = "Cargo.toml",
    test_inventory_command = "cargo test --workspace --all-targets -- --list",
    replay_manifest = "dogfood-results/accepted-workload-proofs.json",
  },
  capabilities = [Capability, Capability],
  change_intents = [{
    change = "invalid-change",
    owner = "chaoscontrol-evidence",
    target_state = "experimental",
    evidence_prerequisite = "Missing evidence.",
    non_claims = ["No promotion."],
  }],
  prohibited_current_claims = ["invalid claim"],
} | ProductScope
