## Why

The oracle already applies Antithesis-compatible `must_hit` verdicts, but exploration details drop that metadata. Summary validators then recompute zero-hit verdicts as `unexercised`, rejecting valid must-hit failures and always-or-unreachable passes. Campaign aggregation can likewise replace those verdicts.

## What Changes

- Preserve optional `must_hit` in exploration, dashboard, strict projection and campaign assertion details. r[chaoscontrol.assertion_reports.must_hit]
- Recompute zero-hit verdicts consistently in Rust and Nickel evidence validators; retain historical behavior when the field is absent. r[chaoscontrol.assertion_reports.must_hit_verdict]
- Reject conflicting must-hit metadata during campaign aggregation. r[chaoscontrol.assertion_reports.must_hit_conflict]

## Impact

- Files: exploration report producers, summary validation and campaign merge; evidence summary validator; Nickel assertion-summary contract; existing regression suites and parity reference.
- Consumer: assertion-summary.v2 readers and campaign operators. Adoption is automatic through existing report production; no new runtime service or dependency.
- Maintenance owner: ChaosControl assertion/report maintainers.
- Verification: real oracle-to-summary smoke, positive and negative summary validation, historical absent-field compatibility, campaign conflict tests, focused Cargo and native Cairn gates.
- Non-claims: no hosted platform parity, new assertion identity, replay format or VM execution claim.
