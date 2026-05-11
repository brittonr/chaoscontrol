## Why

The current readiness history still names hosted service, multi-machine fleet, and multi-language SDK gaps because they are common competitor comparisons. Those are not current product goals. ChaosControl's near-term product surface is a Rust-only SDK and single-machine execution with multiple local hypervisors.

## What Changes

- **Scope contract**: Make Rust-only SDK and single-machine multi-hypervisor operation explicit product scope.
- **Gap wording**: Prevent readiness/status surfaces from treating SaaS, multi-machine fleet scheduling, or non-Rust SDKs as active missing features.
- **Next-step framing**: Require generated/local operator surfaces to identify local multi-hypervisor and Rust workload gaps first.

## Capabilities

### Modified Capabilities
- `replay-readiness-operator`: Narrows product-readiness wording around local multi-hypervisor evidence.
- `rust-workload-harness`: Records Rust-only SDK as intentional scope rather than a language-coverage gap.

## Impact

- **Files**: Readiness surface model/report wording, README/status docs, promotion-gate fixtures, Rust workload docs.
- **APIs**: No public runtime API change expected.
- **Dependencies**: None expected.
- **Testing**: Focused evidence model tests, generated readiness report checks, strict OpenSpec validation.
