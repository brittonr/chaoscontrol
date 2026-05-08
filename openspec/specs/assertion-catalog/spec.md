# Assertion Catalog Specification

## Purpose

Defines compile-time assertion registry and runtime catalog transmission to enable coverage analysis of unexercised property assertions in ChaosControl guests.

## Requirements
### Requirement: Compile-time Assertion Registry

The SDK MUST maintain a compile-time catalog of all assertion declarations using distributed slice collection.

#### Scenario: Assertion macro registration

- GIVEN an assertion macro is expanded in guest code
- WHEN the guest binary is compiled
- THEN a static catalog entry MUST be generated containing assertion ID, message, type, file, and line number

### Requirement: Catalog Transmission

The SDK MUST transmit the complete assertion catalog to the VMM during guest initialization.

#### Scenario: Guest startup catalog send

- GIVEN a guest binary contains assertion catalog entries
- WHEN the guest reaches setup_complete phase
- THEN the catalog MUST be serialized and sent to VMM via CMD_SEND_CATALOG hypercall

#### Scenario: Empty catalog handling

- GIVEN a guest binary contains no assertions
- WHEN the guest reaches setup_complete phase  
- THEN an empty catalog MUST be transmitted to maintain protocol consistency

### Requirement: Oracle Pre-population

The PropertyOracle MUST pre-populate assertion records from the received catalog before guest execution begins.

#### Scenario: Catalog-based oracle initialization

- GIVEN the VMM receives a guest assertion catalog
- WHEN the PropertyOracle is initialized
- THEN assertion records MUST be created for all catalog entries marked as unexercised

### Requirement: Coverage Tracking

The PropertyOracle MUST distinguish between exercised and unexercised assertions in coverage reports.

#### Scenario: Exercised assertion tracking

- GIVEN an assertion is registered in the catalog and fires during execution
- WHEN a coverage report is generated
- THEN the assertion MUST be marked as exercised with execution details

#### Scenario: Unexercised assertion reporting

- GIVEN an assertion is registered in the catalog but never fires
- WHEN a coverage report is generated  
- THEN the assertion MUST be reported as unexercised with catalog metadata

### Requirement: Backward Compatibility

The system MUST support guests compiled without assertion catalog capabilities.

#### Scenario: Legacy guest execution

- GIVEN a guest binary without catalog support
- WHEN the guest executes in the VMM
- THEN assertion tracking MUST function normally but without unexercised assertion reporting

### Requirement: No-std Compatibility

The assertion catalog implementation MUST work in no_std guest environments.

#### Scenario: No-std guest compilation

- GIVEN a guest binary compiled in no_std environment
- WHEN assertion macros are expanded
- THEN catalog entries MUST be generated without standard library dependencies
