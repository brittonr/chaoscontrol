# Assertion Json Details Specification

## Purpose

Defines the canonical ChaosControl requirements for assertion json details.

## Requirements
### Requirement: Detail helper functions
The SDK SHALL provide helper functions in a `chaoscontrol_sdk::assert::details` module to build standardized JSON details for common assertion patterns. Helpers MUST cover node state, log operations, network events, and fault conditions.

#### Scenario: Node state details
- **WHEN** a guest calls `details::node(id, term, role)` 
- **THEN** the returned `serde_json::Value` contains keys "node_id", "term", and "role" with the provided values

#### Scenario: Log operation details
- **WHEN** a guest calls `details::log(index, term, length)`
- **THEN** the returned value contains keys "log_index", "term", and "log_length"

#### Scenario: Network event details
- **WHEN** a guest calls `details::network(from, to, delivered)`
- **THEN** the returned value contains keys "from", "to", and "delivered"

#### Scenario: Composing details
- **WHEN** a guest calls `details::node(0, 5, "leader").merge(details::log(3, 5, 10))`
- **THEN** the returned value contains all keys from both helpers

### Requirement: Standard key constants
The SDK SHALL define string constants for all standard detail keys to prevent ad-hoc naming. Constants MUST be in a `details::keys` submodule.

#### Scenario: Key constants available
- **WHEN** a guest imports `chaoscontrol_sdk::assert::details::keys`
- **THEN** constants NODE_ID, TERM, ROLE, LOG_INDEX, LOG_LENGTH, COMMIT_INDEX, PEER_ID, MESSAGE_TYPE, FROM, TO, DELIVERED are available

### Requirement: Oracle detail display
The oracle and triage system SHALL store the last-seen details for each assertion and include them in failure reports. The `format_report()` output MUST show details for any assertion that failed.

#### Scenario: Failed assertion shows details
- **WHEN** an `always` assertion fails with details `{"node_id": 2, "term": 5, "commit_index": 3}`
- **THEN** the triage report includes those details alongside the assertion message and failure count

#### Scenario: Passing assertion omits details
- **WHEN** an `always` assertion passes on every evaluation
- **THEN** the report summary does not include per-evaluation details (only count)

### Requirement: Backward compatibility
The detail helpers SHALL be optional. Existing code using raw `&json!({})` or `&json!({"custom": val})` MUST continue to compile and work without modification.

#### Scenario: Raw json still accepted
- **WHEN** a guest passes `&json!({"my_key": 42})` to `assert::always`
- **THEN** the assertion fires and the oracle records the raw details

#### Scenario: Empty details still accepted
- **WHEN** a guest passes `&json!({})` to any assertion function
- **THEN** the assertion fires with empty details and no runtime error

### Requirement: Guest call site migration
All existing assertion call sites in chaoscontrol-raft-guest, chaoscontrol-guest, and chaoscontrol-net-guest SHALL be updated to use standardized detail helpers. Migration MUST NOT change assertion semantics or IDs.

#### Scenario: Raft guest migrated
- **WHEN** the raft guest's safety assertions are updated
- **THEN** they use `details::node()` and `details::log()` helpers instead of ad-hoc json
- **AND** assertion IDs (FNV hash of message) remain identical
