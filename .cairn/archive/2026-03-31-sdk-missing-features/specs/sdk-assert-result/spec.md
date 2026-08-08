## ADDED Requirements

### Requirement: cc_assert_always_ok macro
The SDK SHALL provide a `cc_assert_always_ok!` macro that asserts a `Result` is `Ok` every time the assertion is reached. On `Err`, the macro SHALL auto-capture the error's `Debug` representation in assertion details as `{"error": "<debug_string>"}`.

#### Scenario: Ok result passes
- **WHEN** `cc_assert_always_ok!(Ok::<i32, String>(42), "operation succeeds")` is evaluated
- **THEN** the assertion SHALL fire with `condition = true` and empty details

#### Scenario: Err result fails with captured error
- **WHEN** `cc_assert_always_ok!(Err::<i32, String>("timeout".into()), "operation succeeds")` is evaluated
- **THEN** the assertion SHALL fire with `condition = false` and details containing `{"error": "\"timeout\""}`

### Requirement: cc_assert_sometimes_ok macro
The SDK SHALL provide a `cc_assert_sometimes_ok!` macro that records whether a `Result` is `Ok`, with the property that it must be `Ok` at least once across all runs. On `Err`, details SHALL contain the error's `Debug` representation.

#### Scenario: Ok result recorded
- **WHEN** `cc_assert_sometimes_ok!(Ok::<i32, &str>(1), "write succeeds")` is evaluated
- **THEN** the assertion SHALL fire with `condition = true`

#### Scenario: Err result recorded
- **WHEN** `cc_assert_sometimes_ok!(Err::<i32, &str>("fail"), "write succeeds")` is evaluated
- **THEN** the assertion SHALL fire with `condition = false` and details containing the error

### Requirement: cc_assert_always_err macro
The SDK SHALL provide a `cc_assert_always_err!` macro that asserts a `Result` is `Err` every time the assertion is reached. On `Ok`, the macro SHALL auto-capture the ok value's `Debug` representation in details as `{"ok_value": "<debug_string>"}`.

#### Scenario: Err result passes
- **WHEN** `cc_assert_always_err!(Err::<i32, &str>("expected"), "rejects invalid input")` is evaluated
- **THEN** the assertion SHALL fire with `condition = true` and empty details

#### Scenario: Ok result fails with captured value
- **WHEN** `cc_assert_always_err!(Ok::<i32, &str>(42), "rejects invalid input")` is evaluated
- **THEN** the assertion SHALL fire with `condition = false` and details containing `{"ok_value": "42"}`

### Requirement: cc_assert_sometimes_err macro
The SDK SHALL provide a `cc_assert_sometimes_err!` macro that records whether a `Result` is `Err`, with the property that it must be `Err` at least once across all runs. On `Ok`, details SHALL contain the ok value's `Debug` representation.

#### Scenario: Err result recorded
- **WHEN** `cc_assert_sometimes_err!(Err::<i32, &str>("fail"), "error path exercised")` is evaluated
- **THEN** the assertion SHALL fire with `condition = true`

#### Scenario: Ok result recorded
- **WHEN** `cc_assert_sometimes_err!(Ok::<i32, &str>(1), "error path exercised")` is evaluated
- **THEN** the assertion SHALL fire with `condition = false` and details containing the ok value

### Requirement: Result macros accept explicit details
All four Result macros SHALL accept an optional third argument for explicit details: `cc_assert_always_ok!($result, $msg, $details)`. When provided, the explicit details SHALL be used instead of auto-captured error/ok values.

#### Scenario: Explicit details on always_ok
- **WHEN** `cc_assert_always_ok!(result, "msg", &json!({"node": 1}))` is evaluated
- **THEN** the assertion SHALL use the provided details regardless of Ok/Err

### Requirement: Result macros evaluate expression once
All Result macros SHALL evaluate the `$result` expression exactly once by binding to a local variable.

#### Scenario: Side-effecting result expression
- **WHEN** `cc_assert_always_ok!(channel.try_recv(), "receive succeeds")` is evaluated
- **THEN** `try_recv()` SHALL be called exactly once

### Requirement: Result macros Debug trait bounds
The auto-capture form (without explicit `$details`) SHALL require `E: Debug` for `_ok` macros and `T: Debug` for `_err` macros. The explicit `$details` form SHALL NOT impose these bounds.

#### Scenario: Error type without Debug with explicit details
- **WHEN** `cc_assert_always_ok!(result_with_non_debug_error, "msg", &json!({}))` is evaluated
- **THEN** the code SHALL compile without error

### Requirement: Result macros catalog registration
Each Result macro SHALL register an assertion catalog entry. The `_ok` and `_err` always-variants SHALL use `CATALOG_KIND_ALWAYS`. The `_ok` and `_err` sometimes-variants SHALL use `CATALOG_KIND_SOMETIMES`.

#### Scenario: always_ok catalog entry
- **WHEN** a binary containing `cc_assert_always_ok!(r, "my check")` is compiled
- **THEN** the catalog SHALL contain an entry with message `"my check"` and kind `CATALOG_KIND_ALWAYS`

### Requirement: Result macros prelude re-export
All four Result macros SHALL be re-exported from `chaoscontrol_sdk::prelude`.

#### Scenario: Available via prelude
- **WHEN** guest code uses `use chaoscontrol_sdk::prelude::*`
- **THEN** `cc_assert_always_ok!`, `cc_assert_sometimes_ok!`, `cc_assert_always_err!`, and `cc_assert_sometimes_err!` SHALL be available
