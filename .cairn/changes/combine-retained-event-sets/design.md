## Context

Union deduplicates by source position and retains the earliest branch's projections. Both predicate composition and temporal correlation already share admission, work, moments and receipt rules. Intersect/difference must follow that identity rather than treating derived strings as source identity.

## Decision

Use two explicit binary variants with input and other fields; nest Intersect for multiple operands. Evaluate both children even when the left side is empty. Charge one logical operation per materialized row of each operand in addition to child evaluations. Admit right-hand projected envelopes before membership so invalid hidden projections cannot silently disappear. Sort only the owned right vector by source identity/sequence using borrowed comparisons, then binary-search each left position and retain it in place when membership agrees with the selected operator. No Cartesian join, key arrays, extra hash sets or row clones during combination. Preserve original left order and final first-wins duplicate precedence. Existing final row admission remains unchanged.

Validate both descendants under shared structural limits and regex cache. Combine child moments with the existing binary Temporal rule: differing direct child moments are ambiguous; explicit outer UpTo retains its expression-scoped behavior and does not retroactively cap either operand. Receipts bind operator/both expressions/all supplied source events, including nonselected right-side observations. This is source-position set algebra, not full upstream synthetic-field equality or multiverse history.

## Verification

Capture passing replay baseline, inspect exported enum references, admit native gates, and reproduce unsupported variants. Compare both operations with an independent membership oracle over shuffled/multi-source/empty/disjoint/overlapping/identical operands. Verify left projections versus conflicting right projections, duplicate precedence, hidden right admission/parsing, both-child node/depth/work bounds, result limits, moments and query/source/result receipt drift. Exercise actual CLI successes and failures without partial artifacts. Run replay tests, denied-warning Clippy, workspace compilation, formatting, Nickel scope/evidence checks and scoped Cairn gates; sync the tested requirement and keep the change active. Preserve and exclude the user's pointer test and report unrelated existing traceability failures.
