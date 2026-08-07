# Assert Raw Specification

## Purpose

Define the registered generic assertion macro and its structured identity rules.

## Requirements

### Requirement: AssertionKind enum

The SDK SHALL expose `AssertionKind` with `Always`, `Sometimes`, `Reachable`, and
`Unreachable` variants. The enum SHALL support compile-time catalog registration.

#### Scenario: Enum variants select assertion semantics

- **WHEN** a variant is used with `cc_assert_raw!`
- **THEN** it SHALL select the matching assertion command and stable `u8` discriminant
- **AND** the event SHALL bind to the registered descriptor fingerprint and catalog token.

#### Scenario: Enum is const-compatible

- **WHEN** `AssertionKind::Always.to_catalog_kind()` is called in a const context
- **THEN** it SHALL return the stable Always discriminant.

### Requirement: cc_assert_raw macro

The SDK SHALL expose `cc_assert_raw!`. The macro SHALL register an automatic
structured descriptor at compile time and emit only a catalog-bound event.
Runtime-computed kinds and public raw assertion functions are unsupported.

#### Scenario: Catalog registration

- **WHEN** `cc_assert_raw!(AssertionKind::Always, condition, "message")` is compiled
- **THEN** the SDK SHALL add the complete descriptor to `ASSERTION_CATALOG`
- **AND** the compact `u32` value SHALL remain a non-authoritative transport alias.

#### Scenario: Macro with details

- **WHEN** `cc_assert_raw!(AssertionKind::Sometimes, cond, "msg", &json!({"k": "v"}))` runs
- **THEN** the event SHALL carry the validated catalog token and fingerprint
- **AND** bounded details SHALL be forwarded to the oracle.

#### Scenario: Identity resolution fails

- **WHEN** the macro cannot resolve its exact registered descriptor
- **THEN** it SHALL fail closed without sending an unbound assertion event.

### Requirement: Explicit integer assertion APIs are absent

The SDK MUST NOT expose public `assert_raw`, `assert_raw_with_id`, or other
explicit-`u32` assertion functions or macros.

#### Scenario: Old source API is used

- **WHEN** source code calls a removed integer assertion API
- **THEN** compilation SHALL fail
- **AND** no compatibility adapter SHALL map the call to strict evidence.

### Requirement: no_std compatibility

When the `full` feature is disabled, `cc_assert_raw!` SHALL remain a no-op macro
with the same arguments.

#### Scenario: No-op in no_std mode

- **WHEN** a crate compiles `cc_assert_raw!` without the `full` feature
- **THEN** the macro SHALL compile and produce no assertion transport.
