## ADDED Requirements

### Requirement: Fresh workload promotion gate [r[fresh-workload-promotion-gate]]
The replay-readiness static gate MUST fail closed when a workload is presented as supported without an accepted manifest proof, bounded anti-claims, and the generated experimental/unproven surface classifications that prevent overclaiming fresh workload authoring.

#### Scenario: Accepted manifest and report agree [r[fresh-workload-promotion-gate.manifest-report-agree]]
- GIVEN the accepted workload proof manifest and generated readiness status report
- WHEN the promotion gate runs
- THEN every supported workload row maps to exactly one accepted manifest proof with a unique assertion ID
- AND fresh workload authoring remains classified as experimental until a new accepted proof is committed

#### Scenario: Report-only promotion fails closed [r[fresh-workload-promotion-gate.report-only-fails]]
- GIVEN a readiness status report that lists a supported workload missing from the accepted manifest
- WHEN the promotion gate runs
- THEN it exits nonzero and reports the unsupported workload promotion

#### Scenario: Anti-claim removal fails closed [r[fresh-workload-promotion-gate.anti-claim-fails]]
- GIVEN an accepted manifest with missing or weakened anti-claim text
- WHEN the promotion gate runs
- THEN it exits nonzero before the readiness surface can be promoted
