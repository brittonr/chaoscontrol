let P = import "../../contracts/evidence/profile-primitives.ncl" in
let Prior = import "../production-readiness-closure-20261015/attempts.ncl" in
let Attempt = {
  number | P.integer_between 21 25,
  correction | String,
  status | [| 'planned, 'passed, 'failed, 'blocked |],
  evidence | Array String,
} in
{
  schema = "chaoscontrol.production-readiness-authorized-followup-attempts.v1",
  owner = "chaoscontrol-vmm",
  starting_revision = "b33ecb476745f540248514dbec09514927692d67",
  previous_ledger = "dogfood-results/production-readiness-closure-20261015/attempts.ncl",
  previous_used_attempts | std.contract.Equal 20 = Prior.used_implementation_attempts,
  original_maximum_implementation_attempts | std.contract.Equal 20 = Prior.maximum_implementation_attempts,
  maximum_implementation_attempts | std.contract.Equal 25 = 25,
  authorization = {
    source = "Explicit user selection in the active goal conversation",
    selection = "Authorize five additional attempts",
    additional_attempts | std.contract.Equal 5 = 5,
    unchanged_boundaries = [
      "All original acceptance criteria remain required",
      "No sibling edits, host reconfiguration, manual credential access, deployment, or unrelated changes",
      "No weakening of campaign bounds, strict memory/assertion comparison, native readiness, or clean-source release gates",
      "Commit verified changes and push main without force",
    ],
  },
  counting_rule = Prior.counting_rule,
  historical_boundary = "Prior ledgers and receipts remain unchanged. This authorization extends the ceiling prospectively; it does not relabel any failed attempt or old gate verdict.",
  attempts | Array Attempt = [
    {
      number = 21,
      correction = "Retain runtime TSC capability guards and reject an absent/coarse jitter-entropy clock before collector warmup; preserve all declared watchdog, campaign and strict continuation criteria.",
      status = 'passed,
      evidence = [
        "/tmp/chaoscontrol-production-readiness-closure-20261015/followup-kernel20-initcall-visible/stdout.log:2429-2430: previous kernel stops at jent_mod_init",
        "/tmp/chaoscontrol-production-readiness-closure-20261015/followup-kernel20-initcall-visible/stderr.log: operational runnable HostWatchdogTimeout",
        "dogfood-results/production-readiness-followup-20261016/artifact21-observations.json: successful source-built pinned guest kernel and exact BLAKE3-bound artifacts.",
        "dogfood-results/production-readiness-followup-20261016/runtime21-observations.json: passed=true; real watchdog/replacement; all six strict comparison fields match through depth four, including final memory and every admitted assertion outcome.",
        "Paired campaign and selected-replay obligations remain separate: control rejected its assertion summary; planted completed; selected replay emitted no accepted verdict. No overall readiness claim.",
      ],
    },
    {
      number = 22,
      correction = "Maintain meaningful ChaosControl-owned canonicalization inputs and exercise the supported Cairn Preserves producer/freshness validator, including stale-hash rejection; do not copy upstream fixtures or change any readiness applicability.",
      status = 'passed,
      evidence = [
        "Retained matching Cairn preserves_corpus.rs:152-175 exposes update, deliberate execute, and freshness validation for consumer-owned inputs.",
        "/tmp/chaoscontrol-production-readiness-closure-20261015/followup-corpus-stale-preview/execution.json: exit 1, valid=false, stale_count=4, mutated=false.",
        "/tmp/chaoscontrol-production-readiness-closure-20261015/followup-corpus-reviewed-update/execution.json: reviewed supported update --execute succeeded.",
        "/tmp/chaoscontrol-production-readiness-closure-20261015/followup-corpus-export/execution.json: exact Nickel exported the maintained consumer-owned source.",
        "/tmp/chaoscontrol-production-readiness-closure-20261015/followup-corpus-freshness/stdout.log: valid=true, stale_count=0, four cases.",
        "/tmp/chaoscontrol-production-readiness-closure-20261015/followup-corpus-explain/stdout.log: valid=true, four admitted cases.",
        "/tmp/chaoscontrol-production-readiness-closure-20261015/followup-corpus-native-readiness/stdout.log: preserves_corpus_freshness passed=true; overall valid=false and verdict=fail remain authoritative.",
      ],
    },
    {
      number = 23,
      correction = "Select the existing Linux jiffies/notsc boot policy explicitly in reviewed Raft campaign and fresh-replay inputs; preserve the VM count, seeds, faults, every tick/round/attempt bound, and all assertion/replay acceptance. Validate the actual clock override before adopting it.",
      status = 'passed,
      evidence = [
        "/tmp/chaoscontrol-production-readiness-closure-20261015/followup-bootstrap-catalog-hide-false/stdout.log: at tick 10000 VM2 is setup-incomplete with Pending empty catalog; other VMs are Accepted; merged report correctly rejects IneligibleInput.",
        "/tmp/chaoscontrol-production-readiness-closure-20261015/followup-bootstrap-catalog-hide-true/stdout.log: all three guests complete setup by tick 1174, with Accepted 44-site catalogs and an Accepted merged report.",
        "No timeout or bootstrap-bound increase, missing-VM suppression, or assertion-admission relaxation is permitted.",
        "dogfood-results/production-readiness-followup-20261016/campaign23-observations.json: both campaigns have accepted collision-safe assertion catalogs, ten completed rounds, depth ten and twenty measured overlay captures; no retained safety failure or operational timeout.",
        "The control completed in 142.818 seconds and fig8_commit planted campaign in 145.071 seconds under the unchanged 600-second bound. Both retain measured depth-one-through-four overlays.",
        "/tmp/chaoscontrol-production-readiness-closure-20261015/planted-snapshot23/accepted-snapshot-verdict-summary.json: accepted=true, snapshot_backed_reproduced, snapshot_status=valid, reproduced=true, parent depth one.",
        "/tmp/chaoscontrol-production-readiness-closure-20261015/planted-default23/accepted-snapshot-verdict-summary.json: the newly compiled CLI's implicit default also produces the accepted snapshot-backed verdict; both parents were materialized through the supported chunk CLI.",
        "The existing deep replay requirement remains separate; zero deep findings is not promoted into a deep replay success.",
      ],
    },
    {
      number = 24,
      correction = "Exercise the existing planted snapshot probe late enough to retain a depth-three-or-greater exploration finding, then replay every retained deep safety finding. Preserve the ten-round, two-branch, 500-tick, seed-42, three-VM, 600-second campaign bounds; do not change the release cohort or relabel a fig8 algorithm result.",
      status = 'passed,
      evidence = [
        "The active incremental_validation specification requires an actual exploration replay at parent depth three or greater; an empty deep finding inventory is insufficient.",
        "Completed planted21 reached guest tick 9742 within ten rounds; the separate probe threshold is 6000 guest ticks under the same campaign bounds.",
        "This is a separately labeled mechanism-validation input, not a new guest implementation or a change to the existing fail-after-one release profile.",
        "dogfood-results/production-readiness-followup-20261016/deep-probe24-observations.json: ten rounds completed in 265.820 seconds under the unchanged 600-second bound; the accepted catalog retained six planted safety findings, five at parent depth three and one at parent depth four.",
        "Every retained deep safety finding emitted snapshot_backed_reproduced with reproduced=true, exact bug/assertion identity and parent depth, a valid present parent, and verified matching snapshot digest/reference. No host watchdog timeout occurred in the campaign.",
        "The initial throwaway runner invoked env instead of the explorer for export/replay; exit-127 observations remain retained. Only failed export/replay steps were resumed with the actual executable; the successful campaign was not rerun.",
        "The final follow-up profiles reuse the already implemented closure binding, including bitmap coverage at GPA 917504. All seeds, topology, bounds and relevant checkpoint fields match the actual retained commands. The obsolete blind-mode metadata from the initial throwaway projection is explicitly retained as a corrected observation, not a changed runtime or weakened criterion.",
      ],
    },
  ],
  used_implementation_attempts = previous_used_attempts + std.array.length attempts,
  within_limit | std.contract.Equal true = used_implementation_attempts <= maximum_implementation_attempts,
}
