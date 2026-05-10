## MODIFIED Requirements

### Requirement: Coverage Tracking

The PropertyOracle MUST distinguish between exercised and unexercised assertions in coverage reports, and the generated assertion-readiness surface MUST preserve gap evidence before any workload is promoted beyond bounded replay proof.

#### Scenario: Replay probes are checked proof signals, not instrumentation blockers [r[assertion-readiness.replay-probes-not-blockers]]

- GIVEN an accepted workload proof includes a non-passing assertion categorized as `replay-probe`
- WHEN assertion-readiness status and promotion checks are generated
- THEN the system MUST report that assertion as a replay-proof signal outside the ordinary non-passing instrumentation gap count
- AND the promotion checker MUST fail closed if the replay-probe signal count is omitted or weakened
- AND the report MUST preserve anti-claim text that replay-probe visibility is not product parity by itself
