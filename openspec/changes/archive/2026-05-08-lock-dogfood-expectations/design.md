## Context

Accepted workload evidence is committed separately from slow KVM dogfood execution. The wrapper config currently compares to historical summaries, but a historical accepted summary is not always the correct live default probe.

## Decisions

### 1. Lockfile is the source of truth

**Choice:** Store expected default dogfood probe parameters and verdict expectations in a committed JSON lockfile.

**Rationale:** Nix can read JSON directly, Python checks can validate it, and receipts can expose the same expected values without adding new dependencies.

**Alternative:** Keep values duplicated in `flake.nix` and docs. Rejected because that allowed the net default drift.

### 2. Historical evidence remains auditable but not authoritative for defaults

**Choice:** Keep manifest/summary validation for accepted evidence, but compare live wrapper defaults against the expectation lockfile.

**Rationale:** A curated proof may remain valid evidence while a better live smoke default changes. The lockfile records both the live expectation and the linked evidence boundary.

### 3. Fail fast before slow dogfood

**Choice:** Reuse the existing accepted-dogfood-config static gate before optional dogfood execution.

**Rationale:** This catches drift before KVM/kernel work starts.

## Risks / Trade-offs

- **Lockfile staleness** → Mitigated by making wrappers derive from the lockfile and checking generated config against it.
- **Receipt schema growth** → Mitigated by adding optional nested fields under `dogfood` while preserving existing summary fields.
