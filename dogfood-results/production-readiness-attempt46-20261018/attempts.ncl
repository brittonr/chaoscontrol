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
      status = 'passed,
      evidence = [
        "The existing native lineage design admitted the accepted-prefix correction before implementation. Clean ad7e892ad6838872432cc227cb1c88bb6f0ae303 retains independent apply/seal validation and every existing caller.",
        "The staged-prefix/cold-replay regression failed before the repair and passed afterward; final exact-source workspace tests report2204 passed, zero failed and16 ignored. Denied-warning workspace Clippy and formatting passed.",
        "Original600-second control and planted profiles completed ten rounds/twenty branches in245.729 and246.746 seconds with Accepted collision-safe catalogs. All three profiles retain twenty aligned per-VM overlay measurements, but their admitted depth reaches only3, not required4.",
        "The deliberate deep probe completed in319.544 seconds and retained eight depth-three safety findings. Actual Rust checkpoint export admitted the complete inventories; all eight current findings produced snapshot_backed_reproduced with matching identities and valid retained parents.",
        "The current real SI_TIMER watchdog proof passed poison enforcement, controller replacement, full retained-parent and immediate-recapture fidelity, strict memory/execution identities, every admitted assertion outcome, fault ledgers and rounds. Each continuation reports44 passing, zero failing and one unexercised site of45.",
        "The Nickel receipt and BLAKE3 manifest retain source, commands, environments, built executables, immutable kernel/initrd copies, snapshots, assertions, faults, findings, replay verdicts and all prior failures without rewriting historical receipt bytes.",
        "Passed describes this one focused publication-prefix correction, not production readiness. Depth-four campaign completion, selected original historical replay/retention,104 prior native requirement gaps, passing immutable Cairn integration, and fresh passing Nix/all-seven-row matrix verdicts remain mandatory. All46 authorized slots are consumed.",
      ],
    },
  ],
  used_implementation_attempts = previous_used_attempts + std.array.length attempts,
  within_limit | std.contract.Equal true =
    used_implementation_attempts <= maximum_implementation_attempts,
}
