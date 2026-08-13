# Deterministic Schedule Diversity Specification

## Purpose

Make per-branch vCPU interleaving a real input to exploration and to the evidence that exploration produces.

## ADDED Requirements

### Requirement: The explorer generates schedule variants

r[chaoscontrol.schedule_diversity.generation] When schedule diversity is enabled and a VM has more than one vCPU, the explorer MUST generate one optional `ScheduleVariant` per branch through the schedule-aware mutator.

#### Scenario: Diversity enabled on SMP
- GIVEN schedule diversity is enabled and `num_vcpus` is greater than one
- WHEN the explorer builds branch work for a round
- THEN every branch MUST carry the generated variant or an explicit no-variant marker.

#### Scenario: Diversity disabled
- GIVEN schedule diversity is disabled or the VM is single-vCPU
- WHEN the explorer builds branch work
- THEN every branch MUST use default scheduling without error.

### Requirement: Branch execution applies the variant

r[chaoscontrol.schedule_diversity.application] Before a branch run, the shell MUST apply the branch variant to every VM vCPU scheduler. A variant that cannot be applied MUST fail the branch with a typed error.

#### Scenario: Valid variant applies
- GIVEN an admitted variant and a restored snapshot
- WHEN the branch begins
- THEN every VM scheduler MUST run under the variant policy.

#### Scenario: Unsupported strategy
- GIVEN a variant whose strategy or quantum is outside the admitted scheduler bounds
- WHEN the branch begins
- THEN the branch MUST fail closed and record the reason.

### Requirement: Interleaving policy binds to evidence

r[chaoscontrol.schedule_diversity.evidence_identity] The schedule fingerprint, bug report, and replay verdict for a branch MUST bind the variant seed, strategy, and quantum.

#### Scenario: Identical runs share policy bytes
- GIVEN two identical configurations and variants
- WHEN evidence is computed
- THEN the policy identity MUST match.

#### Scenario: Variant identity drifts
- GIVEN evidence whose recorded variant differs from the executed policy
- WHEN replay validation runs
- THEN replay MUST fail closed.

### Requirement: Mechanism validation is a gate

r[chaoscontrol.schedule_diversity.validated_effectiveness] Schedule diversity MUST demonstrate detection of a known scheduling race before a no-bug campaign may claim interleaving coverage.

#### Scenario: Known race is reachable
- GIVEN a fixture race workload with a known triggering interleaving
- WHEN exploration runs with schedule diversity enabled
- THEN the campaign MUST find the bug under at least one declared variant.

#### Scenario: No-bug claim without validation
- GIVEN a campaign completed with schedule diversity enabled but no validated race artifact
- WHEN the campaign reports interleaving coverage
- THEN the report MUST state that the mechanism has not been validated on a known race.

### Requirement: Coverage includes negative cases

r[chaoscontrol.schedule_diversity.validation] Validation MUST pair a positive race-detection fixture with negative fixtures for disabled diversity, single-vCPU, unsupported strategy, and policy-identity drift.

#### Scenario: Closeout validation runs
- GIVEN maintainers intend to treat schedule diversity as search evidence
- WHEN pure, VM, replay, and lifecycle validation runs
- THEN every positive and negative class MUST produce its expected result.
