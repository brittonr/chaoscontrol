## ADDED Requirements

### Requirement: Diagnostic sometimes observations remain progress data

r[chaoscontrol.diagnostic_sometimes.observations]

The SDK diagnostic report MUST retain false sometimes hits in per-site observation and failure counters without counting them in its observed safety-violation counter. Eventual true observations MUST clear the never-successful progress gap; never-successful sites MUST retain that gap. Observed always/unreachable violations MUST retain their existing counts. The diagnostic report MUST remain non-collision-safe and MUST NOT replace strict property verdicts or admission checks.

#### Scenario: Eventual progress

- GIVEN false, true and false hits for one sometimes property
- WHEN the SDK diagnostic report is parsed
- THEN it retains three hits and two false observations, reports no observed violation and has no never-successful progress gap

#### Scenario: Stalled progress

- GIVEN a sometimes site with only false hits
- WHEN the diagnostic report and strict quality gate are evaluated
- THEN raw observations remain present, the diagnostic progress gap remains and the strict gate rejects the never-successful property

#### Scenario: Safety violation

- GIVEN observed always violations or a reached unreachable site
- WHEN the diagnostic report is parsed
- THEN its observed-violation counter still counts those violations and the strict quality gate still rejects an admitted safety violation
