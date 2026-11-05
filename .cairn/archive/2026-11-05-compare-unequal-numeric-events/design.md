## Context

NumberComparison already shares numeric typing and finite IEEE-754 binary64 conversion between details and parsed canonical text. Ordinary Not inverts the whole predicate, including absence and nonnumeric non-matches; that behavior must remain unchanged.

## Decision

Add NotEqual alongside Equal and compare actual != expected only inside the existing admitted numeric pair. Missing paths, null, strings, booleans and containers remain false. Signed zeros compare equal and wide integers retain existing binary64 rounding. JsonTextNumber continues to reject visited malformed/trailing/deep JSON rather than treating it as unequal; short-circuiting and skipped-descendant admission retain existing behavior. Both operands still require admitted finite numbers and confined pointers. No additional allocation or work charge.

## Verification

Capture the passing replay baseline, enumerate exported enum references, admit proposal/design/tasks before implementation, and reproduce unknown NotEqual using the existing comparison matrix. Extend both numeric matrices with positive and negative inequality cases and missing-details coverage. Keep a shared cross-source regression for signed zero, numeric representation equivalence and binary64 precision boundaries. Exercise the actual CLI for both sources, strict typing versus Not(Equal), exact/exhausted work, parse/admission failures, moment composition and query/source/result receipt drift with no partial evaluation artifacts. Run replay tests, denied-warning Clippy, workspace compilation, formatting, Nickel scope/evidence checks and scoped Cairn gates. Sync the implemented requirement, retain the active change, report unrelated existing traceability failures and exclude the user's pointer test from publication.
