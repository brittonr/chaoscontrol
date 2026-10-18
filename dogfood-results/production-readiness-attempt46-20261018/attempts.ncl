let P = import "../../contracts/evidence/profile-primitives.ncl" in
let Prior = import "../production-readiness-attempts41-45-20261018/attempts.ncl" in
let Attempt = {
  number | std.contract.Equal 46,
  correction | String,
  status | [| 'planned, 'passed, 'failed, 'blocked |],
  evidence | Array String,
} in
{
  schema = "chaoscontrol.production-readiness-focused-attempt46.v1",
  owner = "chaoscontrol-vmm",
  starting_revision = "0619a133d4c2279b83418fbada32ffc6e6d802f6",
  previous_ledger = "dogfood-results/production-readiness-attempts41-45-20261018/attempts.ncl",
  previous_used_attempts | std.contract.Equal 45 = Prior.used_implementation_attempts,
  original_maximum_implementation_attempts | std.contract.Equal 20 =
    Prior.original_maximum_implementation_attempts,
  maximum_implementation_attempts | std.contract.Equal 46 = 46,
  authorization = {
    source = "User response in the active goal conversation after the explicit next-correction proposal",
    selection = "do it",
    additional_attempts | std.contract.Equal 1 = 1,
    additional_scope = "One focused owner-local staged-prefix replay correction and its verification through the unchanged campaign contracts",
    unchanged_boundaries = [
      "Every original production-readiness acceptance criterion remains required",
      "No sibling edits, host reconfiguration, manual credential access, deployment or unrelated corrections",
      "Retain exact admission, identity, publication-fence, resource reserve, durability and seal validation",
      "Retain original campaign, replay and release bounds, strict state/assertion comparisons, source cleanliness and passing-report requirements",
      "Reuse the existing snapshot-lineage repair and raft_lineage_proof; use the existing native Cairn change",
      "Preserve all historical ledgers, receipts and retained artifacts; use BLAKE3 for new identities",
      "Commit verified changes and push main without force; no further corrective attempt is implied",
    ],
  },
  counting_rule = Prior.counting_rule,
  historical_boundary = "This separate ledger records new authority; no previous receipt, source identity, command or verdict is rewritten or promoted.",
  attempts | Array Attempt = [
    {
      number = 46,
      correction = "Advance the existing validated ControlSession projection after each accepted staged publication, retaining independent state comparison and final segment sealing, so the next publication does not replay all earlier staged controls.",
      status = 'planned,
      evidence = [
        "The native lineage design records the accepted-prefix correction before implementation; the complete campaign/replay task remains pending.",
        "The retained read-only diagnosis binds actual controls and full state comparisons; its single-pass timing is not a production fix or passing campaign verdict.",
      ],
    },
  ],
  used_implementation_attempts = previous_used_attempts + std.array.length attempts,
  within_limit | std.contract.Equal true =
    used_implementation_attempts <= maximum_implementation_attempts,
}
