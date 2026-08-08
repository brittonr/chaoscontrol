# Design

## Decision

Treat assertions categorized as `replay-probe` and carrying a non-passing verdict as replay-proof signal evidence rather than assertion-readiness promotion blockers. They remain visible and checked in the generated report so operators cannot accidentally hide the proof mechanism.

## Boundaries

- Do not mutate committed accepted-proof `assertions.json` artifacts.
- Do not rerun VM campaigns.
- Do not claim product parity or full Antithesis replacement from zero instrumentation gaps.

## Verification

Use focused `chaoscontrol-evidence` tests, generated report checks, assertion promotion gate/selftest, clippy, OpenSpec validation, and the evidence-contracts Nix check.
