## Context

ChaosControl repeats small security boundaries because each consumer needs a local file or JSON helper. The current file reader protects the final component on supported Unix hosts, but its API starts from an ambient path. JSON scanners and bounded writers also use separate limit types.

The shared repository must separate deterministic admission from authority-bearing I/O.

## Decisions

### Decision: Publish one product-neutral repository

**Choice:** Create `bounded-input` under AGPL-3.0-or-later. Publish independent core, JSON, and standard-library adapter crates from one repository.

**Rationale:** File, JSON, and decompression limits share policy types and failure semantics. Separate crates keep dependencies narrow.

### Decision: Make limits caller-owned

**Choice:** Require a typed `InputLimits` value for every operation. Limits cover source bytes, result bytes, JSON depth, JSON nodes, string bytes, allocation, compressed bytes, and expanded bytes. The shared core has no hidden operational defaults.

**Rationale:** Consumers have different trust and resource budgets. Named limits prevent accidental policy transfer.

### Decision: Keep the core pure

**Choice:** The core evaluates lengths, counters, state transitions, and typed violations over supplied facts or byte slices. It performs no file, environment, clock, process, network, or output access.

**Rationale:** Consumers can check the security policy without filesystem fixtures.

### Decision: Use handle-first file APIs

**Choice:** The standard shell reads already-open file handles. A separate adapter opens relative names beneath an explicit directory capability. Ambient path convenience APIs cannot be the primary security boundary.

**Rationale:** `O_NOFOLLOW` on only the final path component does not define full traversal authority.

### Decision: Keep JSON checks structural

**Choice:** An iterative scanner enforces byte, depth, node, and string limits before deserialization. A bounded writer stops serialization before output exceeds policy.

**Rationale:** Structural admission limits work. It does not establish application schema validity.

### Decision: Bound both sides of decompression

**Choice:** Streaming decompression enforces separate compressed-input and expanded-output limits. It returns a typed limit or codec failure without publishing partial output as complete.

**Rationale:** A small compressed input can expand beyond memory or artifact policy.

### Decision: Preserve stronger consumer authority

**Choice:** `cap-root` retains lexical path admission. `bounded-tree` retains recursive tree observation. Each consumer retains capability acquisition, schema policy, artifact trust, evidence meaning, and lifecycle claims.

**Rationale:** Bounded reads do not transfer those responsibilities.

### Decision: Migrate by parity before deletion

**Choice:** Run old and shared implementations against maintained positive and negative corpora. Remove local code only after accepted values, failures, and limit boundaries agree.

**Rationale:** Small parser differences can weaken a security boundary.

## Risks / Trade-offs

- A portable handle API needs explicit unsupported behavior on platforms without required file guarantees.
- Strict parity can preserve awkward historical errors until a separate compatibility change updates them.
- One limit type can become too broad if optional features do not stay in separate crates.
