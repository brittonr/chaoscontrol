# Design: Refresh Vendored Cairn Policy

## Context

`cairn-policy/default.ncl` is the ChaosControl overlay. It imports `upstream-default.ncl`, which imports `contracts.ncl`, `lifecycle-store-contracts.ncl`, and `nominal-evidence-identities.ncl`. The last two already match Cairn at `fde71b298b61856936c35b701876d4933393f0df`. The projection is committed so the CLI finds it by default.

## Decisions

### 1. One pinned revision

Every imported file comes from one full Cairn commit hash. `UPSTREAM.md` records that hash and the file mapping. No file mixes revisions.

### 2. Closure check

The import is complete only when every file in the overlay's import closure matches the pinned revision byte for byte, or is listed in `UPSTREAM.md` as ChaosControl-owned.

### 3. Projection regeneration

The projection is regenerated with the refresh command already named in the overlay, using the Cairn build at the pinned revision. The committed projection must equal a fresh export.

### 4. Validation findings

Every `cairn validate` finding on an active change is fixed in that change or recorded in this change's tasks with the reason it remains. The first probe found invalid cross-capability requirement markers in `isolate-branch-watchdog-timeouts`, which have been fixed.

## Risks

- A newer policy can reject existing active or archived artifacts. Those findings belong to the owning change and are not suppressed by policy edits.

## Bounded MCP consumer integration

`fixtures/agent-completion/registry.ncl` owns the real portable command set. `inputs.ncl` is a function of measured bindings that emits the original completion contract, closed producer profiles and applicability. It has no default evidence identities and emits no results. `projection.ncl` takes a reviewed sorted regular-file inventory and separates the exact source and dependency projections. The locks, all workspace manifests, build/transport declarations and source inputs remain bound. Generated completion inputs/results and runtime output are excluded to avoid source/subject cycles; historical VM payload exclusions need explicit closure review, never a fabricated retained-artifact claim.

Producer kinds are the closed `cargo_workspace_test`, `cargo_clippy`, `cargo_fmt` and `nix_flake_check` variants. They preserve the exact maintained argv; in particular they do not inject `--offline`, `--target` or `--no-default-features` into the workspace test command. The full Nix check retains its 10,800-second bound; all other per-command bounds retain the clean-gate plan. Formatting/lint/Nix process-role evidence has zero executed-test coverage and cannot establish KVM behavior or release eligibility. The explicitly cleared child environment must contain the reviewed development-shell tools/transport configuration. Nix receives only the reviewed four-line `NIX_CONFIG`, not a host settings mutation.

The compatible cohort migration is one cutover, performed only after the producer revision and CLI/export closure are verified:

1. Import all four mapped policy files from the same immutable public revision; measure identities before any consumer adaptation. Keep the documented removal of the upstream-only `cairn.lifecycle_directory.hidden_default` implementation-marker comment as the sole existing adaptation unless the verified source no longer contains it.
2. Preserve the ChaosControl policy name, refresh command and traceability overlay; add `release_readiness_policy = import "release-readiness-selection.ncl"` through the supported typed field. Keep the preserved corpus selected and record all imported and consumer-owned paths and BLAKE3 identities in `UPSTREAM.md`. Do not write a placeholder pin.
3. Regenerate the committed policy with the matching immutable packaged CLI/export evaluator, then independently require a fresh export match. Keep the import pin distinct from ambient executables and any other Nix source inputs.
4. Observe reviewed source and nonempty dependency projections; derive command/selection/policy identities with Cairn's original canonical identity helpers. Export measured contract/profiles/applicability through the Nickel factory. Plan, review, execute once into a fresh repository-relative output root, and verify the Rust-published manifest before consuming `results.json`.
5. Exercise original CLI and read-only MCP completion admission with the exact selected `--policy`; run positive rows and real failure/drift/invalid-selection controls. Require the JSON verdicts, not process status. Run strict native validation, traceability, projection freshness and readiness against the same projection without omitting original checks.

Native readiness must not recursively become an obligation inside its own MCP completion contract. The seven-row KVM matrix and its Rust-owned receipt validator remain independent mandatory release gates. Neither portable completion nor `mcp_agent_smoke.passed=true` closes historical replay/retention, inaccessible payload, product evidence or clean-source acceptance gaps.
