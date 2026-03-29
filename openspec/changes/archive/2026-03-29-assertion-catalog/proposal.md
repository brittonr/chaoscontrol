# Assertion Catalog

## Why

ChaosControl's PropertyOracle tracks assertion outcomes but only knows about assertions AFTER they execute. This creates a blind spot: if a `sometimes()` assertion is never reached or a `reachable()` path is never explored, the oracle can't report these gaps because it doesn't know the assertions exist.

This prevents comprehensive coverage analysis and leaves potentially critical untested assertions invisible in exploration reports.

## What Changes

- **Compile-time catalog**: Static registry of all assertions using `linkme` distributed slice
- **Startup registration**: Guest sends full assertion catalog to VMM via new hypercall
- **Oracle pre-population**: PropertyOracle knows about all assertions before execution begins
- **Coverage reporting**: Oracle reports both exercised and unexercised assertions

## Capabilities

### New Capabilities

- `assertion-catalog`: Compile-time registry and runtime transmission of all guest assertions to enable coverage analysis of unexercised properties

## Impact

- **Files**: chaoscontrol-sdk (catalog module, macro changes), chaoscontrol-protocol (new hypercall), chaoscontrol-fault (oracle changes), chaoscontrol-vmm (hypercall handler)
- **APIs**: Assertion macros gain catalog registration, PropertyOracle gains pre-population
- **Dependencies**: Add linkme crate to chaoscontrol-sdk
- **Testing**: Verify catalog transmission, oracle pre-population, unexercised assertion reporting