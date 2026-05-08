# Sdk Guidance Specification

## Purpose

Defines the canonical ChaosControl requirements for sdk guidance.

## Requirements
### Requirement: SDK guidance function
The SDK SHALL provide a `guidance(message: &str, distance: f64)` function in a `chaoscontrol_sdk::guidance` module that sends a numeric distance-to-violation hint to the VMM via `CMD_GUIDANCE` hypercall. The `distance` parameter SHALL represent how far the current state is from violating the associated property, where 0.0 means violated and larger values mean farther from violation.

#### Scenario: Guest sends guidance from inside a VM
- **WHEN** guest code calls `guidance("leader count", 2.0)`
- **THEN** the SDK SHALL issue a `CMD_GUIDANCE` hypercall with the assertion ID derived from the message string via `location_id()` and the f64 value `2.0` encoded as little-endian bytes in the result field of the hypercall page

#### Scenario: Guest sends guidance outside a VM
- **WHEN** guest code calls `guidance("leader count", 2.0)` outside a ChaosControl VM
- **THEN** the SDK SHALL silently discard the call (no-op), consistent with other SDK functions in local/noop mode

### Requirement: SDK guidance_with_id function
The SDK SHALL provide a `guidance_with_id(id: u32, distance: f64)` function that sends guidance with an explicit assertion ID, bypassing `location_id()` derivation.

#### Scenario: Explicit ID guidance
- **WHEN** guest code calls `guidance_with_id(0x1234, 1.5)`
- **THEN** the SDK SHALL issue a `CMD_GUIDANCE` hypercall with `id = 0x1234` and distance `1.5`

### Requirement: Guidance no-op stubs
When built with `default-features = false`, the `guidance` and `guidance_with_id` functions SHALL compile as zero-cost no-ops, consistent with the SDK's no_std mode.

#### Scenario: No-op build compiles
- **WHEN** a crate depends on `chaoscontrol-sdk` with `default-features = false`
- **THEN** calls to `guidance()` and `guidance_with_id()` SHALL compile and produce no runtime code

### Requirement: Fault engine handles CMD_GUIDANCE
The fault engine's `handle_hypercall` SHALL handle `CMD_GUIDANCE` by reading the assertion ID from `page.id` and the f64 distance from `page.result` (guest-written, little-endian). It SHALL store the value in a `guidance_values: HashMap<u32, f64>` map, overwriting any previous value for that ID. It SHALL return `(0, STATUS_OK)`.

#### Scenario: Guidance value stored in fault engine
- **WHEN** the fault engine receives a `CMD_GUIDANCE` hypercall with `id = 0xABCD` and distance `3.14`
- **THEN** `guidance_values[0xABCD]` SHALL equal `3.14`

#### Scenario: Guidance value overwritten
- **WHEN** the fault engine receives `CMD_GUIDANCE` with `id = 0xABCD` and distance `3.14`, then receives another with `id = 0xABCD` and distance `1.0`
- **THEN** `guidance_values[0xABCD]` SHALL equal `1.0`

#### Scenario: NaN guidance is stored as-is
- **WHEN** the fault engine receives `CMD_GUIDANCE` with distance `NaN`
- **THEN** the value SHALL be stored without error; consumers are responsible for handling NaN

### Requirement: Guidance prelude re-export
The `chaoscontrol_sdk::prelude` module SHALL re-export `guidance` and `guidance_with_id` from the guidance module.

#### Scenario: Prelude includes guidance
- **WHEN** guest code uses `use chaoscontrol_sdk::prelude::*`
- **THEN** `guidance` and `guidance_with_id` SHALL be available without additional imports

### Requirement: Guidance hypercall payload encoding
The SDK SHALL encode guidance distance by writing the `f64` value as 8 little-endian bytes into the `result` field of the hypercall page before triggering the hypercall. The `command` field SHALL be `CMD_GUIDANCE`, `id` SHALL carry the assertion ID, and `payload_len` SHALL be 0 (no string payload).

#### Scenario: Wire format
- **WHEN** SDK sends `guidance("prop", 42.0)`
- **THEN** the hypercall page SHALL have `command = 0x07`, `id = location_id("prop")`, `payload_len = 0`, and bytes at offset 0x10 SHALL be `42.0f64.to_le_bytes()`
