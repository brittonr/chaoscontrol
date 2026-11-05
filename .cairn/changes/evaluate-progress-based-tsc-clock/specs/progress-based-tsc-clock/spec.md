## ADDED Requirements

### Requirement: Progress-based TSC mode stays opt-in and gated

r[chaoscontrol.tsc_progress.mode_gate]

The virtual TSC MUST default to per-exit advancement. A progress-based mode MAY be
selected only by an explicit profile value, MUST consume the same guest-only
retired-instruction progress the scheduler admits, MUST NOT read host clocks, and
MUST remain unavailable while the PMU prerequisite retains
`claims_pmu_support = false`. Selecting the mode MUST NOT change scheduling
decisions or the admitted guest time surface.

#### Scenario: Default unchanged
- GIVEN a profile without the progress mode
- WHEN guest time advances
- THEN advancement remains per VM exit or quantum with no retired-count sampling

#### Scenario: Gated selection rejected
- GIVEN a profile that selects the progress mode while PMU support is unadmitted
- WHEN the profile is admitted
- THEN selection is rejected rather than silently downgraded

### Requirement: Progress mode preserves snapshot and timer consistency

r[chaoscontrol.tsc_progress.snapshot_consistency]

A snapshot MUST record the TSC mode, base, scale, and last admitted retired count.
A restore MUST either resume the same mapping or reject the snapshot; it MUST NOT
re-anchor silently. PIT programming and HLT deadline handling MUST derive from the
same mapping, and any halt advance MUST be an explicit recorded bound.

#### Scenario: Cross-mode restore rejected
- GIVEN a snapshot written under per-exit mode
- WHEN it is restored by a profile selecting progress mode
- THEN restoration is rejected with a distinct outcome

#### Scenario: Same-mode restore resumes
- GIVEN a snapshot written under progress mode
- WHEN two continuations restore it and advance the same retired work
- THEN the resulting TSC values match at the recorded boundaries
