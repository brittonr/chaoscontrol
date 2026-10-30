## Context

History v1 (completion-order `OperationHistory`, SHA-256 digest) and history v2 (`SemanticHistory`, BLAKE3 identity) already exist with pure checkers. Consumers had no single provider entry point and `validate_semantic_report` checked only shape and bindings.

## Decisions

### Decision: Dispatch on exact schema version without conversion

**Choice:** `import_json` accepts only `schema_version` 1 or 2. Version 1 decodes into the typed `OperationHistory` and keeps its SHA-256 digest; version 2 is fully admitted before import succeeds. Export writes the original schema.

**Rationale:** Silently migrating v1 to v2 would change identity and evidence class. Exact dispatch keeps both identities stable.

### Decision: Validate reports by recomputation

**Choice:** Report validation reruns the deterministic bounded checker on the bound history and compares verdict, witness, invalid proof, unknown proof, and reduction exactly.

**Rationale:** A forged state hash or unsupported-model `valid` claim can be shape-correct. Recomputation is the only check that binds the retained proof to the history.

### Decision: Keep effects in the CLI shell

**Choice:** The contract module is pure. The CLI owns bounded file reads (regular files, 16 MiB), writes into existing parent directories, and readback.

**Rationale:** This preserves the semantic checker's functional core.

## Risks / Trade-offs

- Recomputation costs one bounded checker run per validation. The profile bounds already limit that work.
- Reference status in a report remains metadata. It is not evidence that an external reference tool ran.
