# Assert Raw Specification

## Purpose

Defines the canonical ChaosControl requirements for assert raw.

## Requirements
### Requirement: AssertionKind enum

This requirement MUST be satisfied by the corresponding ChaosControl implementation and validation evidence.
The SDK SHALL expose a public `AssertionKind` enum with variants `Always`,
`Sometimes`, `Reachable`, and `Unreachable`. The enum SHALL be `Copy`, `Clone`,
`Debug`, `PartialEq`, `Eq`, and usable in `no_std` contexts. It SHALL provide a
`const fn to_catalog_kind() -> u8` method that maps to the corresponding
`CATALOG_KIND_*` constant.

#### Scenario: Enum variants match protocol commands
- **WHEN** each `AssertionKind` variant is used in `assert_raw_with_id`
- **THEN** it SHALL dispatch to the corresponding `CMD_ASSERT_ALWAYS`,
  `CMD_ASSERT_SOMETIMES`, `CMD_ASSERT_REACHABLE`, or `CMD_ASSERT_UNREACHABLE`
  protocol command

#### Scenario: Enum is const-compatible
- **WHEN** `AssertionKind::Always.to_catalog_kind()` is called in a const context
- **THEN** it SHALL return `CATALOG_KIND_ALWAYS` (0)

### Requirement: assert_raw function

This requirement MUST be satisfied by the corresponding ChaosControl implementation and validation evidence.
The SDK SHALL expose `assert_raw(kind, cond, message, details)` that dispatches an
assertion of the given kind through the hypercall transport. The assertion ID SHALL
be computed from the message via `location_id()`, matching the behavior of the typed
assertion functions.

#### Scenario: Always assertion via assert_raw
- **WHEN** `assert_raw(AssertionKind::Always, true, "safety", &json!({}))` is called
- **THEN** the oracle SHALL record a true evaluation for assertion "safety" with kind Always

#### Scenario: Sometimes assertion via assert_raw
- **WHEN** `assert_raw(AssertionKind::Sometimes, false, "liveness", &json!({}))` is called
- **THEN** the oracle SHALL record a false evaluation for assertion "liveness" with kind Sometimes

#### Scenario: Reachable via assert_raw ignores condition
- **WHEN** `assert_raw(AssertionKind::Reachable, false, "path reached", &json!({}))` is called
- **THEN** the oracle SHALL record the point as reached (condition value is ignored)

#### Scenario: Unreachable via assert_raw ignores condition
- **WHEN** `assert_raw(AssertionKind::Unreachable, true, "dead code", &json!({}))` is called
- **THEN** the oracle SHALL record the point as reached (immediate failure)

### Requirement: assert_raw_with_id function

This requirement MUST be satisfied by the corresponding ChaosControl implementation and validation evidence.
The SDK SHALL expose `assert_raw_with_id(kind, cond, id, message, details)` that
accepts an explicit assertion ID instead of computing it from the message. This
enables frameworks that maintain their own ID schemes.

#### Scenario: Explicit ID used in transport
- **WHEN** `assert_raw_with_id(AssertionKind::Always, true, 42, "custom", &json!({}))` is called
- **THEN** the hypercall SHALL use assertion ID 42 (not `location_id("custom")`)

### Requirement: cc_assert_raw macro

This requirement MUST be satisfied by the corresponding ChaosControl implementation and validation evidence.
The SDK SHALL expose a `cc_assert_raw!` macro that:
1. Registers the assertion site in the linkme catalog at compile time
2. Dispatches through `assert_raw_with_id` at runtime

The macro SHALL accept the same trailing-comma tolerance as other `cc_assert_*!` macros.

#### Scenario: Catalog registration
- **WHEN** `cc_assert_raw!(AssertionKind::Always, condition, "message")` is compiled
- **THEN** a `CatalogEntry` with the message and kind SHALL be added to the `ASSERTION_CATALOG` distributed slice

#### Scenario: Macro with details
- **WHEN** `cc_assert_raw!(AssertionKind::Sometimes, cond, "msg", &json!({"k": "v"}))` is called
- **THEN** the details JSON SHALL be forwarded to the oracle

### Requirement: no_std compatibility

This requirement MUST be satisfied by the corresponding ChaosControl implementation and validation evidence.
When the `full` feature is disabled, `assert_raw()`, `assert_raw_with_id()`, and
`cc_assert_raw!` SHALL compile as no-ops that accept the same arguments but discard
them, matching the behavior of the existing typed assertion stubs.

#### Scenario: No-op in no_std mode
- **WHEN** the crate is compiled without the `full` feature
- **THEN** `assert_raw(AssertionKind::Always, true, "msg", &())` SHALL compile and do nothing
