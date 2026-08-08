## Why

ChaosControl now has evidence for bounded local KVM multi-hypervisor replay-readiness campaigns. The remaining product-shaped gap is not another local VM run: it is a shared hosted/fleet control surface where multiple machines can coordinate queue work and operator decisions from durable shared state without raw-log scraping or overclaiming Antithesis parity.

## What Changes

- Add a hosted scheduler/decision-store contract for replay-readiness campaigns that coordinates multiple machine identities through shared queue state.
- Require each remote worker lease/run to link queue entry, machine identity, hypervisor worker identity, replay-readiness receipt path, stable summary, and operator decision records.
- Define fail-closed evidence for stale leases, duplicate queue ownership, split-brain decision writes, missing receipt links, and local-only overclaims.
- Keep the first implementation bounded: a deterministic shared-state adapter and two-node/local-loopback harness are acceptable; production SaaS, global scheduling, and universal determinism remain out of scope.
- Update generated readiness surfaces only when hosted/shared evidence exists, preserving explicit non-claims for full product parity.

## Impact

- Files: likely `chaoscontrol-evidence`, scheduler/decision receipt code, `flake.nix`, generated readiness docs, and model tests.
- APIs: shared-state scheduler plan/receipt validation plus worker/decision-store CLI modes or extensions to the scheduler receipt CLI.
- Testing: pure shared-state and validator tests, negative stale/split-brain fixtures, a bounded two-node or loopback integration receipt, readiness report check, OpenSpec validation, and focused Nix packaging.
