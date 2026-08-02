## Context

The accepted assertion design uses complete canonical descriptors, domain-separated BLAKE3 fingerprints, catalog tokens, strict event binding, and descriptor-aware merge. The implementation places reusable semantics beside ChaosControl wire and runtime code.

Extraction must follow completion of the assertion-conflict change. Moving unstable identity code first would create two migration targets.

## Decisions

### Decision: Publish three narrow crates

**Choice:** The `assertion-semantics` repository publishes `assertion-model`, `assertion-catalog`, and `assertion-oracle` as independent AGPL crates.

**Rationale:** Guest code can depend on the model without pulling host report dependencies. Catalog and oracle users can select only required layers.

### Decision: Preserve exact identity bytes

**Choice:** Move the existing identity version, descriptor normalization, canonical encoding, domain label, BLAKE3 fingerprint, and token derivation without changing their bytes.

**Rationale:** Extraction must not change replay or report identity.

### Decision: Support `no_std` plus `alloc` in the model

**Choice:** `assertion-model` uses deterministic collection-independent data types and supports the guest environment. Standard-library error and serialization adapters remain optional.

**Rationale:** The guest SDK needs the same authoritative identity implementation as the host.

### Decision: Keep catalog and oracle logic pure

**Choice:** Catalog insertion, completion, collision classification, event resolution, run transitions, and report merge consume in-memory values and return typed results. They perform no transport, persistence, logging, or clock access.

**Rationale:** These rules form the testable semantic core.

### Decision: Keep transport and policy in adapters

**Choice:** ChaosControl retains hypercall command numbers, wire framing, guest registration macros, KVM dispatch, snapshot codecs, report files, and readiness gates.

**Rationale:** Those surfaces belong to the deterministic VMM and its compatibility contract.

### Decision: Keep Valence ownership explicit

**Choice:** The shared repository can export assertion facts for a Valence adapter. It does not define Valence roles, canonical Evidence IR, evidence promotion, or stack provenance.

**Rationale:** Assertion identity is a domain identity, not a replacement for canonical stack evidence.

### Decision: Use dual-run parity before cutover

**Choice:** Evaluate old and shared implementations over canonical descriptor, catalog, event, snapshot, and report fixtures. Require exact bytes and equal typed outcomes before deleting local logic.

**Rationale:** A source move cannot silently create new property identities.

## Risks / Trade-offs

- Stable exact bytes constrain future cleanup of descriptor fields.
- AGPL model code changes the guest embedding license after the unified-license change.
- A general oracle API can become policy-heavy if ChaosControl readiness decisions leak into it.
- Cross-repository releases require coordinated protocol and adapter version checks.
