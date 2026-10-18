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
  historical_boundary = "All earlier ledgers, receipts, source revisions, artifacts and verdicts remain unchanged. This ledger replaces its prospective entries with scoped observed outcomes, not whole-requirement or release completion.",
  attempts | Array Attempt = [
    {
      number = 36,
      correction = "Preserve the Raft guest's applied application prefix across volatile protocol restarts; reject changed or truncated applied history. The initial read-only Nix investigation made no implementation change, so this uses the still-unstarted authorized slot rather than creating attempt 41.",
      status = 'passed,
      evidence = [
        "target/pr40/correction36-application-before: raft_consumer_retains_applied_history_across_restart failed before the correction; the exact observation remains retained.",
        "target/pr40/correction36-application-after and correction36-application-clippy: all 109 Raft library tests and denied-warning Clippy passed.",
        "Clean 7dace3fbd283c61e4e8ea0ed59da7e143fbc36fb: the rebuilt current guest passed the strict three-controller watchdog proof with all six comparison fields equal, no safety failure, 44 passed and one unexercised of 45 admitted sites.",
        "This does not complete the whole SMR acceptance contract, an arbitrary-protocol proof, or production readiness.",
      ],
    },
    {
      number = 37,
      correction = "Correct restore preflight, queue-state admission and post-memory restore poisoning in the existing VM/runtime owners; exercise complete backend continuation and concurrent host perturbation without weakening accepted comparison fields.",
      status = 'passed,
      evidence = [
        "target/pr40/integrated-vm-runtime passed all eight actual KVM runtime scenarios, including repaired-descriptor rejection after a post-memory KVM_SET_PIT2 failure.",
        "target/pr40/integrated-workspace, integrated-clippy and integrated-format passed; subsequent exact clean-source workspace, denied-warning Clippy and formatting passed again on 7dace3fbd283c61e4e8ea0ed59da7e143fbc36fb.",
        "These scoped checks do not close exhaustive architecture-value preflight, PMU continuation, or every remaining snapshot/SMP/virtio requirement.",
      ],
    },
    {
      number = 38,
      correction = "Preserve admitted process-instance carriers across extraction, checkpointing, deduplication and reproduction; apply retained variants and use newly observed markers rather than inherited cumulative history in frontier proposals.",
      status = 'blocked,
      evidence = [
        "Workspace and actual CLI/contract positive and rejecting paths passed; original diagnostic-policy and invalid-quantum failures remain separate observations.",
        "The prior debug-profile control, planted and probe commands exhausted their unchanged 600-second bounds; those timeouts were not guest bugs.",
        "Restored release-profile commands on exact clean 7dace3fbd283c61e4e8ea0ed59da7e143fbc36fb retain valid one-round checkpoints, then stop with Frontier Adapter Campaign BoundExceeded. No ten-round/depth-four current campaign or non-vacuous fresh deep replay is inferred.",
        "The six original deep findings and selected original probe reproduced separately using the verified original executable. Current readers still reject their missing mandatory failure state; no healthy defaults or legacy substitution were added.",
      ],
    },
    {
      number = 39,
      correction = "Tighten actual retained SMR operation admission, process role/identity binding and evidence ownership-family admission while retaining the existing product boundaries and unsupported whole-requirement gaps.",
      status = 'passed,
      evidence = [
        "Integrated workspace tests and denied-warning Clippy passed; actual contract/registry commands passed after the retained failing invocation was corrected.",
        "target/pr40/corrected-cohort/consumer-artifacts/summary.json records the current actual registry admission, each required-family omission rejection, and process-instance empty/boundary/duplicate/token/cardinality controls; all passed.",
        "No standalone SMR replay shell, privileged eBPF corpus, exact upstream kernel receipt, or complete Wasm/profile/OCI requirement is inferred from these scoped controls.",
      ],
    },
    {
      number = 40,
      correction = "Resolve the real combined-barrier compiler/caller prerequisite and exercise the corrected clean cohort through the unchanged Cargo, native, Nix, watchdog and seven-row release consumers.",
      status = 'blocked,
      evidence = [
        "target/pr40/corrected-workspace and corrected-clippy retain the actual missing vm_memory::GuestMemory trait-import failure. The corrected integrated barrier and current clean-source workspace, Clippy and formatting passed.",
        "The current-guest watchdog proof on source 2aa212f87021391f03717b86089330db0c8967d8 matched memory/execution but exposed raft-adapter-rollback in all three continuations; attempt 36 corrected that real consumer defect, not a weakened assertion.",
        "Native traceability on exact clean 7dace3fbd283c61e4e8ea0ed59da7e143fbc36fb reports 266 of 382 referenced requirements, 116 mandatory gaps and zero dangling coverage references. Native readiness and MCP smoke remain nonpassing; official Preserves freshness passes.",
        "The exact-root native invocation is retained separately from the earlier runner that explicitly targeted the older checkout. Frozen-source native validation also rejects two out-of-scope SMR references in task documentation; delivery corrects only that documentation scope and keeps the frozen-source failure.",
        "Required full Nix and all-seven-row clean-source release observations retain their terminal commands, unchanged bounds and actual producer verdicts in observations.json; no exit-only or synthetic success is accepted.",
        "All 40 authorized corrective slots are consumed. Evidence/documentation closure and unchanged-source verification do not authorize another product correction.",
      ],
    },
  ],
  used_implementation_attempts = previous_used_attempts + std.array.length attempts,
  within_limit | std.contract.Equal true = used_implementation_attempts <= maximum_implementation_attempts,
}
