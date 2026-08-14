## Why

ChaosControl's assertion descriptor, canonical identity, catalog admission, oracle transitions, and report merge rules are useful outside the VMM. They currently span protocol, SDK, fault, explore, and evidence crates. Wire command constants and KVM transport obscure the reusable semantic core.

A shared AGPL repository can provide one assertion model without transferring Valence evidence ownership or ChaosControl runtime claims.

## What Changes

- Establish an `assertion-semantics` repository under AGPL-3.0-or-later.
- Extract `no_std` plus `alloc` assertion descriptors, canonical bytes, and domain-separated BLAKE3 fingerprints.
- Extract pure catalog construction, completion, conflict rejection, token binding, and namespace-aware merge logic.
- Extract pure run and oracle transitions plus deterministic report aggregation.
- Keep hypercall command numbers, guest transport, KVM dispatch, persistence, rendering, and readiness policy in ChaosControl adapters.
- Preserve the current strict identity format and rejection behavior through parity fixtures.
- Define a Valence adapter that wraps assertion facts without making this repository a canonical stack-identity owner.

## Impact

- **Source candidates**: protocol identity, canonical, and admission modules; SDK catalog code; fault oracle and report merge; explore and evidence adapters.
- **New repository**: `assertion-semantics` with model, catalog, and oracle crates.
- **License dependency**: ChaosControl's unified AGPL change must complete before former Apache guest crates depend on this repository.
- **Compatibility**: canonical bytes, fingerprints, catalog tokens, strict report fields, and legacy rejection must remain stable during extraction.
- **Claims**: the repository checks assertion identity and transition consistency. It does not prove the asserted property or release eligibility.
