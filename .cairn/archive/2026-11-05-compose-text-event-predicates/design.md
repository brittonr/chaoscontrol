## Context

Standalone EventQuery::Contains and Matches use field_value, validate_field and validate_text. EventPredicate already supports bounded All/Any/Not with complete prevalidation and per-visited-child charging. Reusing the existing string selection rules avoids a second field namespace, string coercion policy or regex engine.

## Decisions

Add Contains { field, text } and Matches { field, text } directly to EventPredicate. Contains is literal case-sensitive substring selection; Matches is exact string equality, not a regex. Use field_value to borrow canonical or derived strings without allocating/copying row payloads or search strings. Validate both variants using existing nonempty/control-free field/text byte bounds. Unknown well-formed field names address derived fields; absent values are non-matches. Canonical field precedence remains unchanged. Not inverts the full result, so negated missing-field matches are true. Keep standalone query operators as supported independent query forms, not deprecated aliases.

## Verification

Capture baseline tests and a failing-before serialized compound text regression. Test warning/error selection with exclusions, Unicode/case sensitivity, exact-vs-substring boundaries, missing-field negation, canonical-field precedence over derived fields, Map-derived string selection, exact/over field and needle byte bounds, malformed skipped branches, exact/insufficient operation budgets and receipt needle drift even when selected rows remain identical. Exercise the actual query/validate CLI. Run replay tests, Clippy, workspace compilation, formatting, scoped contracts and Cairn gates. Preserve and exclude the user's existing pointer test from publication.
