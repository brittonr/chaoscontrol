## ADDED Requirements

### Requirement: ISOP covers are independently checked

r[chaoscontrol.isop_experiment.cover_soundness]

A candidate cover over the frozen inclusion variables MUST be rejected unless every
cube is sufficient over all completions in the domain, every failing row is
covered, and every cube owns at least one failing row not covered by the others.
The checker MUST derive these facts from the frozen outcome table and a shared cube
match rule only, without reusing the merge or selection recurrence. The experiment
MUST cover multi-fault interactions, redundant alternatives, negative literals with
non-monotone outcomes, duplicate same-time entries as distinct variables, and
constant outcome functions.

#### Scenario: Broken covers rejected
- GIVEN a verified cover and a fault-injected modification
- WHEN a cube is dropped, widened beyond the failing rows, or added redundantly
- THEN coverage, sufficiency, or irredundancy fails respectively

#### Scenario: Finite-model summary
- GIVEN a controlled failing table
- WHEN an irredundant cover is reported
- THEN each cube is sufficient within the declared domain and the report states it is not a unique cause or minimum set

### Requirement: ISOP integration stays gated on admitted branch outcomes

r[chaoscontrol.isop_experiment.gated_integration]

No operator surface MAY use the experiment until branch-local outcomes are
independently verified and indefinite rows are classified and excluded. This
experiment MUST NOT produce a VM, replay, schedule-row, or correctness claim.

#### Scenario: No premature surface
- GIVEN the counterfactual witness prerequisite is incomplete
- WHEN this change is accepted
- THEN only the detached experiment exists and no normal CLI, receipt, or operator surface is added
