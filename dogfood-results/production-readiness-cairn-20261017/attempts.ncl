let P = import "../../contracts/evidence/profile-primitives.ncl" in
let Prior = import "../production-readiness-followup-20261016/attempts.ncl" in
let Attempt = {
  number | P.integer_between 26 30,
  correction | String,
  status | [| 'planned, 'passed, 'failed, 'blocked |],
  evidence | Array String,
} in
{
  schema = "chaoscontrol.production-readiness-authorized-cairn-attempts.v1",
  owner = "chaoscontrol-vmm",
  starting_revision = "96c2fdbaa9bf19cd626b71480c7355c19a561072",
  cairn_starting_revision = "d2c1dd7867350b787cfffe0e311fd20115238339",
  previous_ledger = "dogfood-results/production-readiness-followup-20261016/attempts.ncl",
  previous_used_attempts | std.contract.Equal 25 = Prior.used_implementation_attempts,
  original_maximum_implementation_attempts | std.contract.Equal 20 = Prior.original_maximum_implementation_attempts,
  maximum_implementation_attempts | std.contract.Equal 30 = 30,
  authorization = {
    source = "Explicit user selection in the active goal conversation",
    selection = "Permit bounded Cairn work",
    additional_attempts | std.contract.Equal 5 = 5,
    additional_repository_scope = "Cairn producer support for genuine dependency observation and native Nix payload admission, plus its supported ChaosControl integration",
    unchanged_boundaries = [
      "All original acceptance criteria remain required",
      "No unrelated sibling edits, host reconfiguration, manual credential access, deployment, or unrelated changes",
      "No weakening of campaign bounds, strict memory/assertion comparison, native readiness, or clean-source release gates",
      "Use dedicated Cairn worktree, repository-owned lifecycle and dependency-update workflows; commit verified changes and push without force",
    ],
  },
  counting_rule = Prior.counting_rule,
  historical_boundary = "Prior ledgers, receipts, gate verdicts and raw artifacts remain unchanged. The extension is prospective and supplies no passing evidence by itself.",
  attempts | Array Attempt = [
    {
      number = 26,
      correction = "Implement genuine bounded dependency-file identity observation and exact local producer applicability rechecks; preserve every declared dependency, required command and publication obligation.",
      status = 'failed,
      evidence = [
        "Cairn dedicated worktree starts at d2c1dd7867350b787cfffe0e311fd20115238339 on change/chaoscontrol-producer-readiness-20261017.",
        "/home/brittonr/git/OnixResearch/cairn/target/cc-producer26/baseline/execution.json: unchanged focused baseline exited zero without timeout; 32 producer core tests passed.",
        "/home/brittonr/git/OnixResearch/cairn/target/cc-producer26/{proposal,design,tasks}-gate/stdout.log: all three native foundation reports have valid=true, verdict=PASS, and no issues.",
        "Integrated producer core verification passed 38 tests. CLI dependency positive, stale, preflight, post-child drift and unavailable-file cases passed, but the aggregate local suite failed: the special-file fixture exceeded Linux SUN_LEN, and the pre-existing live-repository developer loop reports two missing license-boundary evidence links. Exact outputs remain in /home/brittonr/git/OnixResearch/cairn/target/cc-producer26/{core-verify,cli-verify,traceability-diagnostic}/.",
      ],
    },
    {
      number = 27,
      correction = "Replace process-only Nix evaluation with the fixed real offline no-link JSON build grammar and admit the native derivation/output payload independently; retain build-only meaning, zero test-coverage claims, and original completion/manifest consumers.",
      status = 'passed,
      evidence = [
        "The native Cairn support-local-producer-applicability change defines the Nix payload cutover before implementation.",
        "Nix 2.31.6 build.cc emits native built paths only after Installable::build realizes outputs; dry-run and evaluator values are not this contract.",
        "Real CLI native Nix conformance passed all three cases: successful realization reached original completion/manifest consumers with zero test coverage, a failed realization stayed incomplete, and an exit-zero empty build set stayed nonpassing. /home/brittonr/git/OnixResearch/cairn/target/cc-producer26/nix-cli-verify/execution.json records exit 0 without timeout and stdout records 3 passed.",
        "Independent direct CLI proof passed without authoring result rows: native build plus nonempty dependencies, original completion, manifest verification, missing/dependency-drift rejection, stale verification rejection, restored identity acceptance, and a real failed build with exit-zero nonpassing producer receipt. Exact commands, verdicts and BLAKE3 artifacts are in /home/brittonr/git/OnixResearch/cairn/target/cc-producer26/direct-smoke/summary.json.",
      ],
    },
    {
      number = 28,
      correction = "Make the real filesystem Unix-socket rejection fixture independent of checkout path length using a held directory descriptor; preserve all dependency observation bounds and admission cases, and retain unrelated baseline failures explicitly.",
      status = 'passed,
      evidence = [
        "Attempt 26 reached the SUN_LEN error before its invalid-member and aggregate-bound cases; no passing evidence is claimed for those unexercised cases.",
        "The live Cairn traceability diagnostic reports only cairn.license_boundary.policy_reuse_grant and cairn.license_boundary.policy_reuse_scope missing; licensing policy and unrelated accepted requirements are outside this bounded producer change.",
        "The corrected real filesystem boundary regression passed all malformed, no-follow, special-file, count, per-file and aggregate-byte cases: /home/brittonr/git/OnixResearch/cairn/target/cc-producer26/fixture28-verify/stdout.log records 1 passed, zero failed; no admission bound changed.",
      ],
    },
    {
      number = 29,
      correction = "Migrate the entire four-file vendored policy closure and its generated projection to one immutable compatible Cairn producer cohort, preserve the consumer overlay and full required-check semantics, and exercise actual native consumer reports without fabricated completion rows.",
      status = 'blocked,
      evidence = [
        "The new producer CLI rejects the retained b6759012 policy projection because its outcome-machine workflow profile is absent. Actual validation/coverage fail, while release-readiness exits zero with nonpassing report fields; observations are in /home/brittonr/git/OnixResearch/cairn/target/cc-producer26/downstream-{validation,traceability,readiness}/.",
        "The existing native refresh-vendored-cairn-policy change and cairn-policy/UPSTREAM.md own the complete import-closure, compatible export, freshness and validation procedure. Migration alone cannot resolve genuine missing product requirements or pass release readiness.",
        "Cairn producer mechanisms are retained at public branch change/chaoscontrol-producer-readiness-20261017, commit 35e6bd4c4d6e9d5a3639a0e70ca922faf3b94a48. Its aggregate required gates are nonpassing; no gated matching CLI/export cohort or complete ChaosControl MCP inputs were established. attempt29-closure.json retains the incomplete probe and records restoration of the two intermediate imports to the existing compatible consumer HEAD. Overlay, dependency pins and generated projection were not migrated.",
      ],
    },
    {
      number = 30,
      correction = "Make producer conformance fixtures automatically owned and cleaned, remove checkout/TMPDIR-dependent Unix-socket path limits, and isolate the actual developer-loop consumer fixture from unrelated repository license coverage while retaining every independent repository check and failure.",
      status = 'passed,
      evidence = [
        "The first unchanged full gate run passed Clippy and formatting, but two pre-existing producer unit fixtures hit SUN_LEN; Nix path-source acquisition rejected an owned generated socket left under the worktree target tree. The original real-repository developer-loop case also reaches the independent two-link license failure. Logs are retained under /home/brittonr/git/OnixResearch/cairn/target/cc-producer26/final-checks/.",
        "Native repository validation emitted valid=true and no issues (it has no verdict field). Independent native traceability retained its two missing license requirements; Octet rejected pre-existing noncanonical finding paths. No license policy, accepted requirement, native traceability gate, or Octet failure is suppressed by fixture isolation.",
        "The corrected local producer suite passed all 12 cases, including the real developer-loop consumer, special-file and byte bounds, drift and cleanup; all three real native Nix cases passed. The source-projection and held-inode unit regressions passed in both binary targets. No SUN_LEN failure remains in this observed command. fixture30-checks/workspace-tests/stdout.log retains the actual names and outcomes.",
        "The entire workspace command later exceeded its unchanged 600-second outer bound after entering evidence_registry; it is not a passing workspace result. Packaged policy freshness and flake checks separately timed out while waiting for the global garbage-collector lock. Native validation, Clippy with warnings denied and formatting passed; independent license traceability and Octet failures remained nonpassing.",
        "post30-direct-smoke/summary.json and mcp-consumer-smoke/summary.json record passing positive/negative actual CLI controls using retained executable BLAKE3 c3e613d967d417a50c8bb804ed97d035f1a307221934cddf6268ff8ceca88968. The failed-build MCP control exits zero with valid=false; read-only MCP executes no checks. These are mechanism controls, not ChaosControl full required-contract completion or mcp_agent_smoke readiness.",
        "The Nickel-validated native change evidence/producer-receipt.ncl keeps all eight required gate outcomes and computes all_required_checks_passed=false. The change remains active, its final validation task unchecked, and no main integration or archive was performed. All 30 authorized corrective attempts are consumed; documentation/evidence closure does not authorize another repair.",
      ],
    },
  ],
  used_implementation_attempts = previous_used_attempts + std.array.length attempts,
  within_limit | std.contract.Equal true = used_implementation_attempts <= maximum_implementation_attempts,
}
