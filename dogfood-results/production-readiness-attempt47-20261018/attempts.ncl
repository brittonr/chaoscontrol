let Prior = import "../production-readiness-attempt46-20261018/attempts.ncl" in
let Attempt = {
  number | std.contract.Equal 47,
  correction | String,
  status | [| 'planned, 'passed, 'failed, 'blocked |],
  evidence | Array String,
} in
{
  schema = "chaoscontrol.production-readiness-focused-attempt47.v1",
  owner = "chaoscontrol-vmm",
  starting_revision = "ff117902977d80cf8886949dbceca2c6db551270",
  previous_ledger = "dogfood-results/production-readiness-attempt46-20261018/attempts.ncl",
  previous_used_attempts | std.contract.Equal 46 = Prior.used_implementation_attempts,
  original_maximum_implementation_attempts | std.contract.Equal 20 = Prior.original_maximum_implementation_attempts,
  maximum_implementation_attempts | std.contract.Equal 47 = 47,
  authorization = {
    source = "Explicit user selection for depth_retention_authority after the read-only completed-campaign diagnosis",
    selection = "Authorize attempt 47",
    additional_attempts | std.contract.Equal 1 = 1,
    additional_scope = "One ChaosControl-owned proposal/retention-diversity correction, native Cairn review boundary, failing-before/passing-after regression and unchanged-bound campaign/replay verification",
    unchanged_boundaries = [
      "Every original production-readiness acceptance criterion remains required",
      "No sibling edits, host reconfiguration, manual credential access, deployment or unrelated corrections",
      "Preserve marker/assertion/snapshot identities, Campaign rank/admission/publication contracts, strict controller comparisons and passing-report requirements",
      "Retain ten rounds, two branches, 600-second campaign bounds and every original replay/release bound; no depth relabeling or diagnostic-policy fallback",
      "Reuse the implemented snapshot-lineage repair, existing raft_lineage_proof and existing native Cairn change",
      "Preserve all historical ledgers, receipts and raw artifacts; use BLAKE3 for new evidence identities",
      "Commit verified changes and push main without force; no additional corrective slot is implied",
    ],
  },
  counting_rule = Prior.counting_rule,
  historical_boundary = "This separate ledger records new authority. No previous receipt, source identity, command or verdict is rewritten or promoted.",
  attempts | Array Attempt = [
    {
      number = 47,
      correction = "Prevent repeated same-branch logical marker observations from monopolizing the bounded frontier while retaining every observed marker event and exact representative marker context.",
      status = 'planned,
      evidence = [
        "The read-only diagnosis binds three completed seed42 traces: all50 retained candidates come from round1's one probe, and none of1400/1384/1092 admitted depth3 proposals survive original-score retention.",
        "Native marker requirements retain logical identity independently of instance refs and require repeated identical instances to collapse; shared rank, selection and capacity-pruning parity remain unchanged.",
        "This authorization does not establish a corrected policy, depth-four campaign, passing replay or release verdict. Other native, upstream, historical-artifact and host-owned gate prerequisites remain mandatory.",
      ],
    },
  ],
  used_implementation_attempts = previous_used_attempts + std.array.length attempts,
  within_limit | std.contract.Equal true = used_implementation_attempts <= maximum_implementation_attempts,
}
