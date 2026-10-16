# Cairn Policy Source

The policy source files come from [OnixResearch/cairn](https://github.com/OnixResearch/cairn) at revision `b6759012db4dab0158936e995b7bad39ac10465b`.

Imported files:

- Cairn `cairn-policy/contracts.ncl` becomes `cairn-policy/contracts.ncl`.
- Cairn `cairn-policy/default.ncl` becomes `cairn-policy/upstream-default.ncl`.
- Cairn `cairn-policy/lifecycle-store-contracts.ncl` becomes `cairn-policy/lifecycle-store-contracts.ncl`.
- Cairn `cairn-policy/nominal-evidence-identities.ncl` becomes `cairn-policy/nominal-evidence-identities.ncl`.

`cairn-policy/default.ncl` is the ChaosControl-owned overlay. It sets the local policy name, refresh command, and traceability profile.

Regenerate the projection with:

```text
cairn policy export --source cairn-policy/default.ncl --output cairn-policy/generated/cairn-policy.json
```

This record proves source identity only. It does not prove policy correctness, consumer compatibility, or release readiness.

## Consumer-owned adaptation and compatibility boundary

`cairn-policy/contracts.ncl` is a ChaosControl-owned adapted copy of the recorded upstream file: it omits only the upstream implementation-marker comment for `cairn.lifecycle_directory.hidden_default`. That identifier belongs to Cairn's accepted specifications, not ChaosControl's requirements. The `SafeRelativePath` predicate and all other imported bytes remain unchanged. The other three imported files remain unmodified upstream-owned copies. This is not a substitute local requirement and does not alter traceability admission or policy evaluation.

The 2026-10-15 closure audit measured the four original imported files against the retained source for the recorded revision. All four matched byte for byte before this comment-only adaptation. The retained compatible CLI is `/nix/store/bwjb9hzl3xrndwxbg48ck624pdyakf3p-cairn-0.1.0/bin/cairn`; that store path is a local evidence locator, not a portable dependency source.

The newer installed source documents `setup plan`, preview-first `setup apply`, and `setup check`. Its upgrade procedure requires a pinned source bundle whose `upgrade_from` names the installed cohort and explicitly does not migrate consumer-owned policy. No retrieved published revision or bundle establishes a supported update for this overlay. The public GitHub revision endpoint returned HTTP 404 during the audit. No dependency pin was changed.

Both retained and newer installed sources have identical `release_readiness.rs` bytes (BLAKE3 `a336e94f95ae2689bfa629beb85b20f880b2f2d677cf008d04ec5fc031ecbfe6`). They unconditionally evaluate `mcp_agent_smoke` using `fixtures/agent-completion/{contract,results}.json` and `preserves_corpus_freshness` using `fixtures/preserves-canonicalization/corpus.json` beneath the consumer root. They expose no downstream applicability selection for these checks. Copying Cairn self-fixtures or suppressing the checks would not establish consumer support.

Upstream must publish a retrievable immutable CLI/policy cohort with a documented consumer-policy update path and explicit, receipt-visible applicability semantics for these two Cairn self-checks (or maintained downstream inputs and corresponding producer validation). Until then their non-pass results remain release blockers. The active policy-refresh change remains active; source identity and a compatible pinned CLI do not prove that the newer-policy migration is complete.

## Repository-owned integration and update procedure

The supported current integration is this repository's four-file vendored import closure, the ChaosControl-owned `default.ncl` overlay, and its committed JSON projection. Keep the import revision distinct from the repository's Nix `cairn` input and from an ambient installed CLI: they are not interchangeable compatibility evidence. Use an explicitly selected compatible CLI and an explicit absolute `--policy` path for closure commands. The retained CLI above is an audit locator only, not a new dependency pin.

For an update, acquire a publicly retrievable immutable upstream revision and its matching CLI/export cohort; review the complete import closure; record imported and consumer-owned files here; preserve overlay and required-check semantics; regenerate and independently check the committed projection; then run native validation, traceability, and release readiness against that exact projection. Do not infer compatibility from a newer executable, a store directory name, or a successful export alone. The refresh lifecycle stays active until its recorded acceptance tasks have evidence.

The inspected newer upstream's bootstrap procedure is separate: `setup plan --root CONSUMER --source-root SOURCES --bundle bundle.json`, non-executing `setup apply` with the same inputs, reviewed `setup apply ... --execute --plan-hash HASH`, and `setup check --root CONSUMER`. It requires the exact CLI artifact declared by the source bundle. An upgrade bundle must name the prior cohort in `upgrade_from`; a changed selected policy needs a new generation locator. Setup does not install the executable or migrate this consumer-owned overlay. These are upstream-supported command shapes, not an available ChaosControl migration cohort or authorization to execute setup here.

### Policy discovery caveat

In the retained CLI, validation, traceability coverage, and release readiness load `--policy` or the working-directory-relative `cairn-policy/generated/cairn-policy.json`; `--root` does not rebase that policy path. Native coverage uses the policy's default profile unless `--profile` is supplied. The repository overlay includes `cairn-policy/*.ncl` as declared-link evidence and accepted `.cairn/specs/*.md` as requirements, so the curated audit is discovered without importing it into the policy expression.

The newer bootstrap documentation scopes project selection to validation, doctor, and gates: explicit `--policy` wins without fallback; otherwise a complete `.cairn/bootstrap/selection.json` resolves beneath the selected root; only an absent declaration permits the old working-directory default. Other command families and MCP retain existing loaders. Do not assume bootstrap selection applies to traceability or release readiness.

Exact parent-run command vectors, required verdict fields, non-claims, and source audit counts are recorded in `dogfood-results/production-readiness-closure-20261015/cairn-native-source-audit.json`. Export the Nickel audit data with the exact evaluator source already declared by `flake.nix`: `nix shell github:nickel-lang/nickel/1320a983e6c3d1e2fb53dd2464b084b4903b1426 --command nickel export --format json cairn-policy/native-traceability-closure-20261015.ncl`. This avoids unrelated development-shell dependencies and its stdout preamble. That data export is not a native traceability/readiness verdict or evidence of the retained Cairn policy-export cohort's compatibility.

### Retained policy-export cohort

The retained CLI's `nickel_export_boundary.rs:15-17` records Nickel Export revision `257fafc1c746f1faf156207043a4c826bfb16d49` and descriptor `nickel-lang-cli-1.16.0`. The official executable is a wrapper: it prepends `/nix/store/5yhl5ar0pnbykzm5f288d7ibxvwwrfwm-nickel-1.16.0/bin` and `/nix/store/6bs4wzpdvyzp9xml6p30wgfza8047xdl-nickel-export-0.1.0/bin`, then executes its `.cairn-wrapped` binary. The retained upstream `flake.nix:77-85` owns that packaged tool closure. Outside-wrapper command lookup is therefore not evidence that policy freshness is blocked; invoke the retained wrapper directly and retain its actual result. The descriptor literal alone is not a measurement of tool identities, nor is the repository's separately reviewed 1.17.0 data exporter a replacement policy cohort. `policy export --check` compares rather than rewrites the committed projection, while its dual-run boundary creates and removes bounded scratch files beneath `target/cairn-nickel-export-observation/<pid>`. Freshness results remain distinct from an upstream overlay migration or downstream applicability admission.

The parent executed `nix run --no-update-lock-file path:/nix/store/nh55gw93v3xdks6bg2h9yq3z44sw18f3-source#cairn -- policy export --source cairn-policy/default.ncl --output cairn-policy/generated/cairn-policy.json --check` from the closure checkout. The retained observation at `/tmp/chaoscontrol-production-readiness-closure-20261015/policy-freshness/execution.json` reports exit 0 without operational timeout; stdout is `policy fresh: cairn-policy/generated/cairn-policy.json`. This exercises the supported retained wrapper closure and establishes current projection freshness for that source cohort only. It does not establish a newer-policy migration, downstream self-check applicability, or release readiness.
