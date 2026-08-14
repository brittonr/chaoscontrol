## Why

ChaosControl has mature local Rust and KVM capabilities, but its documents still contain old scale, architecture, and test claims. Active experimental changes can also look like supported product scope before evidence exists.

## What Changes

- Add a typed product-scope registry with supported, experimental, deferred, blocked, and non-goal states.
- Require active product changes to name their intended scope state and evidence prerequisite.
- Generate factual README and status sections from repository and evidence inputs.
- Fail document freshness validation when generated facts or support labels drift.
- Keep hosted services, cross-machine scheduling, containers, and non-Rust SDKs outside current support.

## Impact

- **Configuration**: a Nickel-owned product scope and document projection contract.
- **Lifecycle**: explicit scope and evidence fields for active product changes.
- **Documentation**: README, architecture status, readiness status, and roadmap sections.
- **Testing**: positive current projections and negative stale, unsupported, blocked, and overclaim fixtures.

## Non-Goals

- No permanent ban on experimental research.
- No promotion from documentation alone.
- No claim that generated counts prove code quality.
