# Molten Collaboration Guest Specification

## Purpose

Run Aspen's installed collaboration-native runtime under bounded ChaosControl VM campaigns without forking its authority or lifecycle driver.

## Requirements

### Requirement: The guest wrapper selects one exact driver and trust

r[chaoscontrol.molten_guest.wrapper] The guest wrapper MUST run the exact Aspen collaboration-native driver case against one independently provisioned source-trust record selected from the kernel command line, and MUST fail closed when the driver or trust is unavailable.

#### Scenario: Trust is selected and the driver completes
- GIVEN an initrd contains the driver, the signed output, and the trust record
- WHEN the cmdline selects that trust
- THEN the wrapper MUST run the exact three-node driver case and retain its exit status.

#### Scenario: Trust is absent or rejected
- GIVEN the cmdline trust path does not exist or admission denies it
- WHEN the wrapper runs
- THEN the wrapper MUST record an unreachable assertion and MUST NOT report a successful workload.

### Requirement: The initrd preserves signed source identity

r[chaoscontrol.molten_guest.initrd] The initrd MUST preserve the signed Aspen output and its closure at their absolute paths, include the matching driver binary, and install the operator-provisioned trust record without rewriting it.

#### Scenario: Output identity is preserved
- GIVEN a signed output and trust record
- WHEN the initrd is assembled
- THEN each closure path MUST exist at the same absolute path and the trust MUST select that output.

#### Scenario: Relocated output is denied
- GIVEN an output copied to a different path than the trust selects
- WHEN the driver starts
- THEN source admission MUST deny the run.

### Requirement: The single-guest campaign executes the installed runtime

r[chaoscontrol.molten_guest.campaign] The first campaign MUST run the driver in one guest under a fixed kernel, initrd, seed, schedule, and set of resource bounds, and MUST retain the driver's canonical observation.

#### Scenario: Single-guest campaign completes
- GIVEN the initrd contains the signed output, trust, and driver
- WHEN a bounded no-fault campaign runs
- THEN ChaosControl MUST retain the driver result and its input identities.

### Requirement: Campaign evidence is bounded and retained

r[chaoscontrol.molten_guest.evidence] Every campaign MUST retain the kernel, initrd, output, trust, driver revision, seed, schedule, SDK events, assertion results, and driver observation, and MUST classify the result as bounded VM campaign evidence.

#### Scenario: Evidence is complete
- GIVEN a campaign run finishes
- WHEN evidence is retained
- THEN the receipt MUST name every input and observation identity and MUST include the driver's non-claim.

### Requirement: Three-guest cohorts remain a separate capability

r[chaoscontrol.molten_guest.cohort] A three-guest cohort MUST start one independently admitted node per guest and MUST NOT reuse a single guest's three-process driver result as cross-VM evidence.

#### Scenario: Single-guest evidence is not widened
- GIVEN only the single-guest campaign completed
- WHEN a three-guest cohort is requested
- THEN the request MUST remain unfulfilled until the per-guest campaign runs.
