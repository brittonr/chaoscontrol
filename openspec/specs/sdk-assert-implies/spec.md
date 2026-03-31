## ADDED Requirements

### Requirement: cc_assert_implies macro
The SDK SHALL provide a `cc_assert_implies!` macro that asserts `precondition → conclusion` as an always-true property. It SHALL desugar to an `always` assertion with condition `!precondition || conclusion`.

#### Scenario: Both true
- **WHEN** `cc_assert_implies!(true, true, "p implies q")` is evaluated
- **THEN** the assertion SHALL fire with `condition = true`

#### Scenario: Precondition false
- **WHEN** `cc_assert_implies!(false, false, "p implies q")` is evaluated
- **THEN** the assertion SHALL fire with `condition = true` (vacuously true)

#### Scenario: Implication violated
- **WHEN** `cc_assert_implies!(true, false, "p implies q")` is evaluated
- **THEN** the assertion SHALL fire with `condition = false` and details containing `{"precondition": "true", "conclusion": "false"}`

### Requirement: cc_assert_implies auto-captures on failure
When the implication is violated (`precondition = true, conclusion = false`), the macro SHALL auto-capture both boolean values in the assertion details as `{"precondition": "<value>", "conclusion": "<value>"}`. When the implication holds, details SHALL be empty `{}`.

#### Scenario: Failure details include evaluated values
- **WHEN** `cc_assert_implies!(node.is_leader(), node.has_log(), "leaders have logs")` fires with `is_leader() = true, has_log() = false`
- **THEN** details SHALL contain `{"precondition": "true", "conclusion": "false"}`

### Requirement: cc_assert_implies accepts explicit details
The macro SHALL accept an optional fourth argument for explicit details: `cc_assert_implies!($precondition, $conclusion, $msg, $details)`.

#### Scenario: Explicit details
- **WHEN** `cc_assert_implies!(p, q, "msg", &json!({"node": 3}))` is evaluated
- **THEN** the assertion SHALL use the provided details object

### Requirement: cc_assert_implies evaluates operands once
The macro SHALL evaluate `$precondition` and `$conclusion` exactly once by binding to local variables.

#### Scenario: Side-effecting precondition
- **WHEN** `cc_assert_implies!(check_and_increment(), result, "msg")` is evaluated
- **THEN** `check_and_increment()` SHALL be called exactly once

### Requirement: cc_assert_implies catalog registration
The macro SHALL register a catalog entry with kind `CATALOG_KIND_ALWAYS`, consistent with its semantics as an always-true property.

#### Scenario: Catalog entry created
- **WHEN** a binary containing `cc_assert_implies!(p, q, "my implication")` is compiled
- **THEN** the assertion catalog SHALL contain an entry with message `"my implication"` and kind `CATALOG_KIND_ALWAYS`

### Requirement: cc_assert_implies prelude re-export
The `cc_assert_implies!` macro SHALL be re-exported from `chaoscontrol_sdk::prelude`.

#### Scenario: Available via prelude
- **WHEN** guest code uses `use chaoscontrol_sdk::prelude::*`
- **THEN** `cc_assert_implies!` SHALL be available without additional imports
