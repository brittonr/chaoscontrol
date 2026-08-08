# Sdk Comparison Details Specification

## Purpose

Defines the canonical ChaosControl requirements for sdk comparison details.

## Requirements
### Requirement: Comparison macros auto-capture values on failure
Each comparison assertion macro (`cc_assert_always_lt!`, `cc_assert_always_le!`, `cc_assert_always_gt!`, `cc_assert_always_ge!`, `cc_assert_always_eq!`, `cc_assert_always_ne!`, and the `sometimes` equivalents) SHALL auto-capture the evaluated left and right values in the assertion details when the condition is false. The details object SHALL contain `{"left": <value>, "right": <value>}` where values are formatted via `Debug`.

#### Scenario: Failed always_lt captures values
- **WHEN** `cc_assert_always_lt!(10, 5, "a < b")` is evaluated
- **THEN** the assertion SHALL fire with `condition = false` and details containing `{"left": "10", "right": "5"}`

#### Scenario: Passing always_lt emits no details
- **WHEN** `cc_assert_always_lt!(3, 10, "a < b")` is evaluated
- **THEN** the assertion SHALL fire with `condition = true` and empty details `{}`

### Requirement: Comparison macros evaluate operands once
Each comparison macro SHALL evaluate `$left` and `$right` exactly once by binding them to local variables before the comparison and detail formatting. This prevents double-evaluation of expressions with side effects.

#### Scenario: Side-effecting operand evaluated once
- **WHEN** `cc_assert_always_lt!(counter.fetch_add(1), 10, "msg")` is evaluated
- **THEN** `counter` SHALL be incremented exactly once

### Requirement: Comparison macros accept explicit details
Each comparison macro SHALL accept an optional fourth argument for explicit details, overriding the auto-captured values. The form is `cc_assert_always_lt!($left, $right, $msg, $details)`.

#### Scenario: Explicit details override auto-capture
- **WHEN** `cc_assert_always_lt!(a, b, "msg", &json!({"context": "custom"}))` is evaluated with `a = 10, b = 5`
- **THEN** the assertion details SHALL be `{"context": "custom"}`, not the auto-captured left/right values

### Requirement: Comparison macros require Debug on operands
The auto-capture form (without explicit `$details`) SHALL require that `$left` and `$right` implement `core::fmt::Debug`. The explicit `$details` form SHALL NOT require `Debug` on the operands.

#### Scenario: Non-Debug type with explicit details compiles
- **WHEN** a type without `Debug` is used as `cc_assert_always_eq!(non_debug_a, non_debug_b, "msg", &json!({}))`
- **THEN** the code SHALL compile without error

#### Scenario: Non-Debug type without details fails to compile
- **WHEN** a type without `Debug` is used as `cc_assert_always_eq!(non_debug_a, non_debug_b, "msg")`
- **THEN** the code SHALL fail to compile with a missing `Debug` trait error
