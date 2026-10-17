let P = import "../../contracts/evidence/profile-primitives.ncl" in
let Prior = import "../production-readiness-extension-20261017/attempts.ncl" in
let Attempt = {
  number | P.integer_between 36 40,
  correction | String,
  status | [| 'planned, 'passed, 'failed, 'blocked |],
  evidence | Array String,
} in
{
  schema = "chaoscontrol.production-readiness-authorized-continuation-attempts.v1",
  owner = "chaoscontrol-vmm",
  starting_revision = "ea76e41acad7a84660f15201e840acb7d7c99446",
  previous_ledger = "dogfood-results/production-readiness-extension-20261017/attempts.ncl",
  previous_used_attempts | std.contract.Equal 35 = Prior.used_implementation_attempts,
  original_maximum_implementation_attempts | std.contract.Equal 20 = Prior.original_maximum_implementation_attempts,
  maximum_implementation_attempts | std.contract.Equal 40 = 40,
  authorization = {
    source = "Explicit user selection in the active goal conversation",
    selection = "Authorize five more attempts",
    additional_attempts | std.contract.Equal 5 = 5,
    additional_scope = "Remaining ChaosControl corrections and supported dependency integration",
    unchanged_boundaries = [
      "Every original acceptance criterion remains required",
      "No sibling edits, host reconfiguration, manual credential access, deployment, or unrelated changes",
      "No weakened checks, campaign bounds, strict memory/assertion comparisons, source cleanliness, or passing-report requirements",
      "Use existing native Cairn changes and the repository-owned dependency-update workflow; commit verified changes and push main without force",
    ],
  },
  counting_rule = Prior.counting_rule,
  historical_boundary = "All earlier ledgers, receipts, source revisions, artifacts and verdicts remain unchanged; this prospective authorization supplies no passing result",
  attempts | Array Attempt = [
    {
      number = 36,
      correction = "Correct repository-owned Nix/source-acquisition and release-wrapper defects diagnosed from the retained operational timeouts; preserve exact dependency authority, row commands and all declared bounds. Admit a Cairn update only if an immutable supported upstream cohort actually passes.",
      status = 'planned,
      evidence = ["Retained full Nix timeout occurred during Raft wrapper derivation evaluation; both timed-out matrix rows retained empty stdout/stderr and no runtime verdict. Current upstream Cairn main/producer branch remain d2c1dd7867350b787cfffe0e311fd20115238339 / 35e6bd4c4d6e9d5a3639a0e70ca922faf3b94a48."],
    },
    {
      number = 37,
      correction = "Complete genuine remaining VM, deterministic SMP/snapshot, virtio and runtime-capacity acceptance gaps beyond the already implemented lineage repair, with consumer-visible behavioral regressions and unchanged strict continuation proof.",
      status = 'planned,
      evidence = ["Reuse the prior 33-ID runtime audit and current 6190343 implementation; source presence, isolated old passes and new markers alone do not establish a complete accepted requirement."],
    },
    {
      number = 38,
      correction = "Complete genuine explorer/campaign, event/fallback/owner replay-carrier and production evidence-admission gaps while preserving all mandatory product authority, snapshot and executed-policy checks.",
      status = 'planned,
      evidence = ["Reuse accepted specifications and retained per-ID audit; all source/test claims remain provisional until the parent verification barrier exercises the actual consumers."],
    },
    {
      number = 39,
      correction = "Complete genuine remaining workload/product boundary acceptance gaps in Wasm, SMR, guest/OCI intake, profile and trace/kernel evidence owners without fabricating observations or weakening accepted resource and provenance contracts.",
      status = 'planned,
      evidence = ["Reuse existing implementations and per-ID boundary audit; unavailable historical artifacts, grants and owner-produced source evidence remain prerequisites, not metadata-only closures."],
    },
    {
      number = 40,
      correction = "Resolve actual combined-barrier compiler/caller prerequisites and complete the corrected-cohort integration proof without weakening specifications, declared commands, timeouts, memory/assertion comparisons, or source cleanliness.",
      status = 'planned,
      evidence = ["target/pr40/corrected-workspace and corrected-clippy retain the missing vm_memory::GuestMemory trait-import error in the real post-memory PIT-restore regression; no passing gate is inferred."],
    },
  ],
  used_implementation_attempts = previous_used_attempts + std.array.length attempts,
  within_limit | std.contract.Equal true = used_implementation_attempts <= maximum_implementation_attempts,
}
