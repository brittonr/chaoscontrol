## Why

The assertion-readiness report currently preserves workload-level gap counts, but operators still have to open raw `assertions.json` artifacts to identify which unhit or non-passing assertions are blocking instrumentation readiness. That makes the next remediation step slower and keeps the triage surface thinner than the promotion gate requires.

## What Changes

- Add operator-facing assertion gap details to the generated assertion-readiness report.
- Keep bounded replay proof and anti-claim language unchanged.
- Preserve the existing count-based promotion gate while surfacing enough stable IDs/messages/categories to target remediation.

## Capabilities

### Modified Capabilities
- `assertion-catalog`: assertion-readiness status includes actionable details for unhit and non-passing assertions.

## Impact

- **Files**: `crates/chaoscontrol-evidence`, `docs/assertion-readiness-status.md`, assertion-catalog spec.
- **APIs**: no SDK API changes.
- **Dependencies**: none.
- **Testing**: focused evidence crate tests, assertion-readiness generator/checker, OpenSpec validation, whitespace checks.
