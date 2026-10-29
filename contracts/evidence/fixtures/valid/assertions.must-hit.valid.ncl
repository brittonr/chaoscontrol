let Contract = import "../../assertion-summary.ncl" in
let Summary = fun assertion => {
  schema = "chaoscontrol.assertion-summary.v2",
  catalog_status = "legacy-ambiguous",
  collision_safe_evidence = false,
  assertions = [assertion],
} | Contract in
let Assertion = fun assertion_kind true_hits false_hits expected_verdict => {
  id = 7,
  message = "must-hit verdict fixture",
  kind = assertion_kind,
  guest = "guest",
  category = "invariant",
  verdict = expected_verdict,
  hit_count = true_hits + false_hits,
  true_count = true_hits,
  false_count = false_hits,
} in
let Required = fun kind true_count false_count verdict =>
  Summary (std.record.update "must_hit" true (Assertion kind true_count false_count verdict))
in
let Optional = fun kind true_count false_count verdict =>
  Summary (std.record.update "must_hit" false (Assertion kind true_count false_count verdict))
in
let Historical = fun kind true_count false_count verdict =>
  Summary (Assertion kind true_count false_count verdict)
in
let identified = import "assertions.identity.valid.json" in
let identified_unhit = std.record.update "must_hit" true (
  std.record.update "hit_count" 0 (
    std.record.update "true_count" 0 (
      std.record.update "verdict" "failed" (std.array.at 0 identified.assertions)
    )
  )
) in
# Required, optional, and absent metadata differ only before an assertion is hit.
{
  required_always_unhit = Required "always" 0 0 "failed",
  optional_always_unhit = Optional "always" 0 0 "passed",
  absent_always_unhit = Historical "always" 0 0 "unexercised",
  required_sometimes_unhit = Required "sometimes" 0 0 "failed",
  optional_sometimes_unhit = Optional "sometimes" 0 0 "passed",
  absent_sometimes_unhit = Historical "sometimes" 0 0 "unexercised",
  required_reachable_unhit = Required "reachable" 0 0 "failed",
  optional_reachable_unhit = Optional "reachable" 0 0 "passed",
  absent_reachable_unhit = Historical "reachable" 0 0 "unexercised",
  required_unreachable_unhit = Required "unreachable" 0 0 "passed",
  optional_unreachable_unhit = Optional "unreachable" 0 0 "passed",
  absent_unreachable_unhit = Historical "unreachable" 0 0 "passed",
  required_always_passed = Required "always" 1 0 "passed",
  optional_always_passed = Optional "always" 1 0 "passed",
  required_always_failed = Required "always" 1 1 "failed",
  optional_always_failed = Optional "always" 1 1 "failed",
  required_sometimes_passed = Required "sometimes" 1 1 "passed",
  optional_sometimes_passed = Optional "sometimes" 1 1 "passed",
  required_sometimes_failed = Required "sometimes" 0 1 "failed",
  optional_sometimes_failed = Optional "sometimes" 0 1 "failed",
  required_reachable_passed = Required "reachable" 1 0 "passed",
  optional_reachable_passed = Optional "reachable" 1 0 "passed",
  required_unreachable_failed = Required "unreachable" 0 1 "failed",
  optional_unreachable_failed = Optional "unreachable" 0 1 "failed",
  identified_required_unhit = (std.record.update "assertions" [identified_unhit] identified) | Contract,
}
