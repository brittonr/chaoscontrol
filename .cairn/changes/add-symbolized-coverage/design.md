## Context

The explorer consumes a coverage bitmap for guidance. The bitmap is deliberately lossy: it hashes edges and asserts states into a fixed region and does not retain the module or address that produced a hit. SanCov exposes guard identifiers, and KCOV exposes program counters, but the repository does not retain a symbol catalog or a loaded-module inventory to interpret either.

A symbolized report therefore needs three separate facts that must not be conflated: the build-time catalog, the runtime module load set, and the observed coverage identifiers. A report is valid only when all three agree.

## Decisions

### Decision: Symbols are build artifacts, not inferred from logs

**Choice:** each instrumented module emits a versioned symbol catalog with its content identity, language, instrumentation kind, function names, source paths, line/column ranges, and the identifiers or address ranges it instruments. Source paths are declared relative to a reviewed root.

**Rationale:** parsing logs or guessing from binary symbols would create unverifiable locations. A catalog is an exact artifact that can be bound and validated.

### Decision: Module load identity joins the two sides

**Choice:** the guest runtime records each loaded instrumented module with its exact content identity and load ordinal. A coverage observation is mapped only when its module identity and identifier are present in the admitted catalog. Unknown modules and unmatched identifiers are counted as unaccounted.

**Rationale:** address ranges and sanitizer identifiers can collide across builds. Identity matching prevents a stale catalog from silently symbolizing a different binary.

### Decision: Mapping is a pure projection

**Choice:** the pure core receives admitted catalogs, loaded-module facts, and coverage observations and returns locations plus explicit unmapped counts. It performs no file, clock, process, network, or environment access.

**Rationale:** the mapping must be testable and replayable without KVM or the filesystem.

### Decision: Bug correlation is descriptive and bounded

**Choice:** for a selected bug cohort, the report MAY rank locations whose coverage differs between failing and control branches. Ranks include the compared branch identities, counts, and bounds. They do not carry a causal label.

**Rationale:** differential coverage is useful triage evidence. Presenting it as cause would overstate what a hit count can establish.

### Decision: Missing symbols fail the symbolized claim, not the campaign

**Choice:** a missing, incomplete, or stale catalog yields an explicit symbolized-report failure for the affected module and preserves bitmap-only coverage. It does not stop the campaign or invent a location.

**Rationale:** coverage guidance remains valid without symbols, and a partial symbol closure must be visible rather than concealed.

## Risks / Trade-offs

- Symbol catalogs can be large; per-module and per-report bounds are mandatory.
- Link-time optimization can erase source mapping; the build must instrument at link time or declare an incomplete catalog.
- Shared libraries and dynamically loaded modules require load-ordinal evidence that the current runtime may not emit yet.
- Differential coverage can reflect workload differences rather than the bug; the report must retain the compared branch facts and avoid causal wording.
