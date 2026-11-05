## ADDED Requirements

### Requirement: Raw reachability identity follows execution obligation

r[chaoscontrol.raw_reachability.obligation]

Raw assertion classification MUST select reachable/unreachable from `must_hit`, not `condition` or `hit`. A required reachable declaration with a false placeholder condition MUST retain the same accepted identity as later true observations. Optional forbidden-path observations MUST retain unreachable identity and violation semantics. Raw IDs, source metadata, counts, sparse sampling and late-site rejection MUST be preserved.

#### Scenario: Required declaration followed by execution

- GIVEN a raw reachability declaration with condition=false, hit=false and must_hit=true
- WHEN the same site later emits condition=true and hit=true
- THEN both records bind one reachable descriptor and the successful observation is retained

#### Scenario: Quiet required path or forbidden execution

- GIVEN a quiet required path or an executed optional forbidden path
- WHEN strict local evidence is evaluated
- THEN the missing required path or actual forbidden-path violation fails while a quiet optional forbidden path does not

#### Scenario: Late unknown site

- GIVEN the catalog is already emitted
- WHEN an undeclared raw reachable site attempts to emit
- THEN it remains rejected without acquiring another site's identity
