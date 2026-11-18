# SpaceWasm Full-ABI Differential Specification

## Purpose

Compare SpaceWasm and the patched Wasmtime 49.0.5 cohort on Molten's `molten.wasm.abi.v1` actor surface with one deterministic hostcall model, without widening the MVP rail or its evidence claims.

## Requirements

### Requirement: The full-ABI lane uses its own typed profile

r[chaoscontrol.spacewasm_full_abi.profile] ChaosControl MUST define a typed full-ABI profile that binds the Mantle SpaceWasm bundle, the patched Wasmtime 49.0.5 embedding cohort, the exact `molten.wasm.abi.v1` import, memory and export surface, the scenario matrix, named bounds and non-claims. The MVP profile MUST NOT admit full-ABI cases.

#### Scenario: Complete matching profile is admitted
- GIVEN a profile that names the exact ABI surface, cohort, scenarios, bounds and non-claims
- WHEN profile admission runs
- THEN ChaosControl MAY schedule full-ABI cases under that profile identity.

#### Scenario: Profile widens or drifts
- GIVEN a profile that adds an import, export, memory shape, scenario class or runtime authority, or omits a non-claim
- WHEN profile admission runs
- THEN it MUST deny the profile before any engine starts.

### Requirement: Evidence inputs come from a release-bound bundle

r[chaoscontrol.spacewasm_full_abi.materialization] Evidence-bearing runs MUST remeasure every required module, descriptor and source member from a Mantle bundle whose producer revision and identity a release-authority publication receipt binds. A locally built or unpublished bundle MUST be classified diagnostic-only, even when its bytes match.

#### Scenario: Published bundle verifies
- GIVEN a release-authority receipt and a bundle whose remeasured identity matches it
- WHEN materialization admission runs
- THEN its full-ABI fixtures MAY run as evidence-bearing cases.

#### Scenario: Bundle lacks publication
- GIVEN a bundle without a matching release-authority receipt
- WHEN materialization admission runs
- THEN cases from it MUST be diagnostic-only and MUST NOT satisfy the evidence class.

### Requirement: Only the exact ABI surface executes

r[chaoscontrol.spacewasm_full_abi.abi] ChaosControl MUST execute only modules whose imports are exactly the declared `molten:hostcall` functions with their declared signatures, which declare exactly one bounded memory and no table, start function or `memory.grow`, and whose exports are exactly the declared ABI set.

#### Scenario: ABI module is admitted
- GIVEN a module with the declared hostcalls, one bounded memory and the four ABI exports
- WHEN module admission runs
- THEN it MAY execute under the full-ABI profile.

#### Scenario: Module requests other authority
- GIVEN a module with an undeclared or duplicate import, a wrong signature, an extra export, a table, a start function, an unbounded or second memory, or `memory.grow`
- WHEN module admission runs
- THEN it MUST be denied before execution with the first violated rule.

### Requirement: Both engines share one deterministic host

r[chaoscontrol.spacewasm_full_abi.host] Both engines MUST call the same pure hostcall model, which validates canonical request bytes, answers from declared scenario replies, writes the reply at the ABI reply offset and records an ordered transcript. No engine MAY receive WASI, filesystem, network, environment, clock, randomness or an undeclared import.

#### Scenario: Hostcall transcripts agree
- GIVEN an admitted scenario and identical declared replies
- WHEN both engines run it
- THEN ChaosControl MUST compare their ordered hostcall transcripts, results, traps and canonical outputs.

#### Scenario: Request is not canonical
- GIVEN a guest request whose bytes are not canonical Preserves
- WHEN the hostcall model receives it
- THEN it MUST record a request trap for that engine without dispatching a reply.

### Requirement: Scenarios cover the ABI boundary classes

r[chaoscontrol.spacewasm_full_abi.scenarios] The run MUST execute the positive and negative ABI, hostcall, memory, trap, stack and cumulative-fuel scenarios against the exact bundle member bytes, and MUST keep each outcome in a ChaosControl-owned receipt.

#### Scenario: Negative boundary is reached
- GIVEN a negative module that targets an output pointer, canonical output, memory budget, non-tail recursion or unreachable trap
- WHEN both engines run it
- THEN both outcomes MUST be recorded with their normalized class, and a class mismatch MUST be reported as a divergence.

### Requirement: Segmented execution preserves cumulative fuel

r[chaoscontrol.spacewasm_full_abi.fuel] Execution segmented by pauses or out-of-fuel resumes MUST match uninterrupted execution in result, transcript and total consumed fuel, and resumption MUST NOT replenish the admitted budget.

#### Scenario: Resume restores budget
- GIVEN a segmented run whose total consumed fuel exceeds the admitted budget or differs from the uninterrupted run
- WHEN fuel comparison runs
- THEN ChaosControl MUST report a fuel mismatch.

### Requirement: Evidence stays bounded and diagnostic

r[chaoscontrol.spacewasm_full_abi.evidence] Full-ABI reports MUST bind the profile, bundle, cohort and scenario identities and MUST keep diagnostic-only, evidence-bearing and divergence classes separate from VM replay and assertion-readiness classes.

#### Scenario: Report overclaims
- GIVEN a report that claims SpaceWasm correctness, engine equivalence, memory safety, conformance, Molten runtime correctness or release eligibility
- WHEN evidence validation runs
- THEN it MUST reject the report.

### Requirement: Full-ABI decisions have a functional core

r[chaoscontrol.spacewasm_full_abi.functional_core] Profile, materialization, module and scenario admission, the hostcall model, observation normalization, comparison, fuel accounting and evidence DTOs MUST be pure; process, filesystem, engine and clock effects MUST stay in the shell.

#### Scenario: Core decision is replayed
- GIVEN the same profile, module bytes and scenario inputs
- WHEN a pure admission or comparison runs again
- THEN it MUST return the same decision without performing I/O.

### Requirement: Validation covers positive and negative paths

r[chaoscontrol.spacewasm_full_abi.validation] The change MUST run focused pure-core tests, positive and negative profile and module fixtures, the scenario matrix when its inputs are admitted, evidence contracts, Cairn validation and the proposal, design and tasks gates.

#### Scenario: Inputs remain unpublished
- GIVEN the Mantle bundle lacks a release-authority receipt
- WHEN validation runs
- THEN evidence-bearing scenario tasks MUST stay open with that blocker, and only pure and diagnostic results MAY be recorded.
