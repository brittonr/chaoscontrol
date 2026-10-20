let Prior = import "../production-readiness-attempt47-20261018/attempts.ncl" in
let Attempt = {
  number | std.contract.Equal 48,
  correction | String,
  status | [| 'planned, 'passed, 'failed, 'blocked |],
  evidence | Array String,
} in
{
  schema = "chaoscontrol.production-readiness-focused-attempt48.v1",
  owner = "chaoscontrol-vmm",
  starting_revision = "4cdef855c9cf534d1287533cd18cf5e6f4a32264",
  previous_ledger = "dogfood-results/production-readiness-attempt47-20261018/attempts.ncl",
  previous_used_attempts = Prior.used_implementation_attempts,
  original_maximum_implementation_attempts = Prior.original_maximum_implementation_attempts,
  maximum_implementation_attempts = Prior.maximum_implementation_attempts + 1,
  authorization = {
    source = "Explicit user selection after the fresh 29329c8 dependency-policy failure and clean-source seven-row KVM pass",
    selection = "Authorize one additional corrective attempt",
    additional_attempts | std.contract.Equal 1 = 1,
    additional_scope = "Repair dependency-policy negative controls for active Radicle sources, retain genuine source/license denial checks, and rerun the required gates",
    unchanged_boundaries = [
      "No bound increases, sibling edits, host changes, manual credential access, or unrelated corrections",
      "Every original production-readiness acceptance criterion and passing-report requirement remains required",
      "Preserve historical receipts and local raw artifacts; use BLAKE3 for new evidence identities",
      "Commit verified changes and push main without force; no further corrective attempt is implied",
    ],
  },
  counting_rule = Prior.counting_rule,
  historical_boundary = "Continue the last numbered ledger without rewriting or retroactively authorizing intervening work. This ledger grants and consumes exactly the single additional slot selected here.",
  attempts | Array Attempt = [{
    number = 48,
    correction = "Remove obsolete Campaign/Choregraph HTTPS source allowances and their no-op negative mutations after the Radicle cutover; retain active source and license denial controls.",
    status = 'passed,
    evidence = [
      "On clean 29329c8, the real Nix dependency-policy gate reports the two obsolete HTTPS sources unmatched, then panics because removing an unused allowance does not deny the locked graph.",
      "The unchanged-source seven-row KVM release matrix on 29329c8 passes with release-eligible, independently validated; this prior pass is not a pass on the new correction.",
      "After the correction, the actual Nix dependency-policy baseline and all four active source-denial/two license-denial controls pass under their unchanged 120-second bound. Local commands and logs are retained under target/pr52/focused-policy.",
      "This status records the focused correction only. Full Nix checks and an exact clean-source seven-row matrix must retain their independent verdicts; no production-readiness claim follows from the policy pass.",
      "Clean 17b8abdbb294f7fdb4da7e680a05e74c8e0542a0 passes native validation, 382/382 traceability with zero missing/dangling, denied-warning workspace Clippy and formatting.",
      "The initial workspace command fails at signal::tests::shutdown_flag_initially_false. The complete no-fail-fast run passes 2224 with zero failures and 16 ignored, without test serialization or skips; both outcomes are retained and the shared-state test risk remains unresolved.",
      "Full Nix checks pass dependency-policy but fail the independent focused Octet check: nightly Cargo panics at package_id_spec.rs:244 and reports integration-failure. No toolchain or sibling correction is made under this slot.",
      "The correction-source matrix passes six rows; fresh-workload-replay fails with direct child teardown timed out, and the independent pinned-evaluator receipt check rejects the blocked report. All 112 retained row artifact identities and lengths are verified. The earlier clean 29329c8 matrix pass remains separate.",
    ],
  }],
  used_implementation_attempts = previous_used_attempts + std.array.length attempts,
  within_limit | std.contract.Equal true = used_implementation_attempts <= maximum_implementation_attempts,
}
