# Bundle Details Evaluation Specification

## Purpose

Defines the `bundle-details-evaluation` capability.

## Requirements

### Requirement: Bundle details precede named condition bindings

r[chaoscontrol.bundle_details.order] Boolean bundle assertions with explicit details MUST evaluate and capture caller details exactly once before binding the named conditions. A named condition MUST NOT shadow the caller identifiers used to evaluate those details. All conditions MUST still be evaluated exactly once in source order.

#### Scenario: Details affect a condition

- GIVEN caller details that update state read by the conditions
- WHEN an always-some, always-any or sometimes-all bundle executes
- THEN the details update precedes condition evaluation, matching the Antithesis full-mode bundle behavior.

#### Scenario: A caller identifier matches a condition name

- GIVEN a details expression referring to a caller variable with the same name as a condition
- WHEN the bundle captures details
- THEN it uses the caller variable rather than the later condition binding.

### Requirement: Merged bundle data preserves caller values

r[chaoscontrol.bundle_details.preservation] Full-mode bundle details MUST preserve unrelated caller keys, overwrite named-condition keys with their evaluated booleans, and leave borrowed caller data unchanged. The owned snapshot MUST be merged without another deep clone. No-full bundles MUST retain their call shapes without serialization or JSON allocation.

#### Scenario: Captured caller data is augmented

- GIVEN caller details with unrelated keys and a preexisting condition-name key
- WHEN the bundle executes in full mode
- THEN the emitted details retain unrelated values and replace the condition-name key without mutating the original details.

#### Scenario: Full features are disabled

- GIVEN a no-full SDK build and details-side effects
- WHEN a boolean bundle executes
- THEN its details expression and all conditions still evaluate once in the declared order without requiring JSON serialization.
