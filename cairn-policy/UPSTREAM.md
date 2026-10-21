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

The newer installed source documents `setup plan`, preview-first `setup apply`, and `setup check`. Its upgrade procedure requires a pinned source bundle whose `upgrade_from` names the installed cohort and explicitly does not migrate consumer-owned policy. No inspected bundle establishes a supported update for this overlay. The public GitHub revision endpoint returned HTTP 404 during the earlier audit; the later SSH retrieval below supersedes repository access as a blocker. No dependency pin was changed.

Both retained and newer installed sources have identical `release_readiness.rs` bytes (BLAKE3 `a336e94f95ae2689bfa629beb85b20f880b2f2d677cf008d04ec5fc031ecbfe6`). They unconditionally evaluate `mcp_agent_smoke` using `fixtures/agent-completion/{contract,results}.json` and `preserves_corpus_freshness` using `fixtures/preserves-canonicalization/corpus.json` beneath the consumer root. They expose no downstream applicability selection for these checks. These paths can hold genuine consumer-owned inputs; they do not require copies of Cairn's self-fixtures.

The follow-up below establishes supported consumer-owned Preserves inputs and a passing freshness check. MCP completion remains blocked: the inspected producer cannot observe nonempty dependency identity sets or emit a passing Nix completion row. Do not fabricate results, omit required commands or dependencies, or narrow ChaosControl's required completion contract to make that check pass. No upstream self-fixtures were copied and no required check was disabled. The active policy-refresh change remains active; current import identity and compatible execution do not prove a newer-policy migration.

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

### Current upstream source inspection

Current main was retrieved read-only over the existing SSH-agent route at `d2c1dd7867350b787cfffe0e311fd20115238339`; no manual credential access or sibling edit was needed. Its `release_readiness.rs` is byte-identical to the retained implementation above. Both self-checks still run unconditionally beneath the consumer root, and the inspected bootstrap contract does not migrate consumer-owned policy. Repository access is not the remaining prerequisite.

`dogfood-results/production-readiness-closure-20261015/cairn-current-upstream-audit.json` binds the exact retrieval commands, immutable source archive, module identity and inspected ranges. The required integration cohort and downstream applicability semantics were not established by that inspection. No upstream update was applied; successful retrieval is not a passing readiness verdict.

### Authorized follow-up: consumer inputs and actual producer limits

The immutable `d2c1dd7867350b787cfffe0e311fd20115238339` CLI built successfully from the retained Git source with the upstream-owned Nix package. The observation is `/tmp/chaoscontrol-production-readiness-closure-20261015/followup-cairn-cohort-build/execution.json`. Its realized source is `/nix/store/daqs219app3gq6zvazgy1yw863sd3198-source`. These are evidence locators, not new product dependency paths. The four-file policy import remains pinned to `b6759012db4dab0158936e995b7bad39ac10465b`; no migration is claimed.

`fixtures/preserves-canonicalization/corpus.ncl` now owns four reviewed ChaosControl cases: a retained parent reference, its clock-jump schedule, operational watchdog classification, and reviewed campaign bounds. `corpus.json` is its generated projection. The legacy snapshot digest is data from the retained reference, not a new identity algorithm; all new expected canonicalization identities are BLAKE3. These cases establish canonicalization only, not successful replay, artifact availability, or readiness. No upstream cases were copied.

Maintain the inputs with the exact evaluator and a compatible explicitly selected Cairn wrapper:

```text
nix shell github:nickel-lang/nickel/1320a983e6c3d1e2fb53dd2464b084b4903b1426 --command nickel export --format json --output fixtures/preserves-canonicalization/corpus.json fixtures/preserves-canonicalization/corpus.ncl
cairn preserves-corpus explain --path fixtures/preserves-canonicalization/corpus.json
cairn preserves-corpus update --path fixtures/preserves-canonicalization/corpus.json
```

For deliberate value changes, inspect the update preview, review and apply `update --execute`, move the producer-computed hashes back into the Nickel source, regenerate, and require `valid=true`, `stale_count=0`, and no mutation from the final update check. Do not silently refresh expected hashes during readiness. The follow-up exercised stale-hash rejection (`valid=false`, four stale cases, exit 1), a reviewed update, Nickel export, admission, and clean freshness (`valid=true`, four cases). Actual native release readiness then reported `preserves_corpus_freshness.passed=true`, but overall `valid=false` and `verdict=fail`; traceability and MCP were not promoted.

The current upstream's `docs/check-evidence-producer.md:64-88` and `crates/cairn-cli/src/check_evidence/run.rs:74-79` narrow the remaining prerequisite. Local profiles support only fixed Cargo/Nix grammars. Nonempty dependency identity sets are rejected before execution because dependency observation is unavailable. The Nix adapter has no admitted native build-payload decoder and therefore emits no passing completion row even after process success. ChaosControl requires real Nix gates and pinned dependencies. A complete consumer completion contract cannot be replaced with a dependency-free Cargo-only smoke. Cairn producer-adapter maintainers must provide those two mechanisms before that contract can supply honest passing MCP completion inputs. This is a producer limitation, not unavailable source access or proof that consumer-authored inputs are unsupported.

### Authorized producer implementation and blocked migration, 2026-10-17

Cairn commit `35e6bd4c4d6e9d5a3639a0e70ca922faf3b94a48` is retained on the public branch `change/chaoscontrol-producer-readiness-20261017`. Its bounded producer now observes real nonempty dependency-file identities and admits the payload from `nix build --offline --no-link --json --file ./check.nix`. Real realization, failed realization and exit-zero empty build-set controls passed. Original completion and manifest consumers accepted the valid rows and rejected drift/failure; Nix evidence remains build-only with zero test-coverage claims. Independent read-only MCP controls accepted retained Rust-produced passing rows and rejected failed-build rows even though both MCP processes exited zero. No upstream self-fixtures or hand-authored completion rows were used.

That mechanism evidence does not establish the full ChaosControl required completion contract or a gated CLI/export migration cohort. Cairn's full workspace command timed out after the producer cases passed; packaged policy freshness and Nix checks waited on the global garbage-collector lock and timed out. Native validation, denied-warning Clippy and formatting passed. Independent native traceability still reports the two license-boundary requirement IDs, and Octet rejects noncanonical finding paths. The Cairn change remains active, its final validation task unchecked, and its branch is not integrated into Cairn main.

Attempt 29 retained its intermediate imported bytes locally and restored the two changed imports to the existing consumer HEAD. The four-file recorded `b6759012db4dab0158936e995b7bad39ac10465b` cohort, its consumer overlay, dependency pins and committed projection remain selected. No half-migrated policy is installed. A supported update still requires the procedure above with one compatible immutable CLI/export cohort and every required consumer verdict; no check is skipped and no genuine missing product requirement is relabeled as annotation debt.

`dogfood-results/production-readiness-cairn-20261017/receipt.ncl` binds the producer source/artifact identities, exact commands, actual positive/negative reports, all eight Cairn gate outcomes, the blocked import probe, residual requirement audit and unchanged release blockers. Its 30-attempt ledger is a separate authorized extension; prior ledgers and receipts remain unchanged. Maintainers: Cairn owns producer/gate prerequisites; ChaosControl owns its full completion inputs, residual product evidence and clean-source release acceptance.

### Direct MCP correction: consumer-owned source preparation, 2026-10-20

The new direct user request is **"fix cairn mcp"**. Its scope is the genuine MCP producer/consumer correction and necessary immutable policy/selection cohort. Historical replay, unavailable payloads and unmet product evidence remain open. This section prepares inputs and invocation; it is not a passing execution record or a new upstream pin. The recorded four-file revision above, current overlay and committed policy remain selected until the parent verifies and installs the replacement cohort.

The old `repository_gate_policy_listed` report inherited Cairn's self-host command list. Its `.#checks.x86_64-linux.slotcar` installable is not a ChaosControl gate. It is not the authority for a ChaosControl completion contract. The actual consumer sources are `AGENTS.md`, the build/release sections of `README.md`, `.github/workflows/ci.yml`, and `contracts/kvm-release/{matrix,profile}.ncl`; the retained operational mapping is `target/readiness-attempt52-20261020/clean-gate-plan.json`. That retained plan's source, output and executable locators remain historical observations, not portable configuration.

`fixtures/agent-completion/registry.ncl` preserves the four full portable commands:

| Consumer check | Exact arguments after measured executable | Deadline | Stdout / stderr bound |
| --- | --- | --- | --- |
| `clean-workspace-tests` | `test --workspace --all-targets --locked --no-fail-fast` | 3,600 s | 8 MiB / 8 MiB |
| `clean-clippy` | `clippy --workspace --all-targets --locked -- -D warnings` | 3,600 s | 1 MiB / 8 MiB |
| `clean-formatting` | `fmt --all -- --check` | 300 s | 1 MiB / 1 MiB |
| `clean-nix-flake-check` | `flake check -L` | 10,800 s | 1 MiB / 64 MiB |

The closed producer kinds are respectively `cargo_workspace_test`, `cargo_clippy`, `cargo_fmt`, and `nix_flake_check`. The old `cargo_test` injects extra target/offline/default-feature restrictions and omits `--no-fail-fast`; the old Nix expression grammar cannot substitute for a full flake check. The corrected cohort must admit the exact kinds, commands, bounds and nonempty observed dependencies. Only the new flake kind gets the separately named 10,800,000 ms maximum; the existing producer kinds retain their safety bounds. Formatting/lint/flake process-role observations have zero executed-test coverage. Their process status is not KVM evidence or release eligibility.

Stream bounds are explicit per-check registry inputs, not the old uniform 1 MiB defaults. The retained full workspace stdout and flake stderr exceed that old limit; source preparation therefore uses the reviewed new-kind bounds above. The producer must admit them with separately named new-kind maxima, leaving legacy caps unchanged. Output overflow, truncation or incomplete payload admission remains nonpassing; larger finite bounds are not evidence that a command passed.

`inputs.ncl` is a parameterized Nickel authoring function, not a receipt. Bind actual Nix-store Cargo/Nix executable paths, BLAKE3 executable bytes, reviewed tool-cohort identities and the explicit admitted development-shell environments. Preserve the clean plan's Cargo environment (`CARGO_BUILD_JOBS=2`, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_INCREMENTAL=0`, and an explicitly reviewed `CARGO_TARGET_DIR`) alongside the required tool/transport environment. Preserve the flake child's exact `NIX_CONFIG` value `min-free = 0\nbuilders =\nsecret-key-files =\nmax-jobs = 1\n`; this sets only the child environment and does not edit host settings. Do not substitute an ambient shell wrapper for an executable identity.

The measured development-shell `PATH` may contain duplicate inherited entries and exceed the producer's unchanged 4,096-byte value bound. Remove duplicate entries in first-occurrence order; preserve every distinct entry and command lookup precedence. Bind that reviewed value explicitly rather than raising the legacy bound, dropping required tools, or inheriting an unrecorded ambient environment. Tool-cohort manifests bind the actual executable bytes, reported version, selected lock identity and explicit environment; their BLAKE3 file identities are runtime bindings, not portable defaults.

Apply `projection.ncl` to a reviewed sorted unique inventory of repository-relative regular source/check inputs. Include the complete build/check closure, all workspace manifests, locks, `.cargo/config.toml`, `flake.nix`, the selected policy and these authoring sources. Its `dependencies` projection includes both locks, all projected workspace manifests and the build/transport declarations. Native observation must admit the full nonempty set (at most 64 dependency files). Explicitly review exclusions for outputs, cancellation controls, measured-binding drivers, generated completion JSON and historical VM payload blobs. Do not include generated contract/profiles/applicability/results in the source subject: those bind the subject and would introduce an identity cycle. Source-projection review does not repair inaccessible retained payloads or prove closure completeness.

After the immutable cohort is verified, import all four mapped policy files from that one revision and preserve the documented consumer-owned comment adaptation if still applicable. Activate `cairn-policy/release-readiness-selection.ncl` through the typed `release_readiness_policy` field in `default.ncl`, preserving the name, refresh command and both traceability profiles. It selects genuine consumer contract/results and the existing four-case Preserves corpus. Record exact upstream/source identities here only after verification, regenerate the policy with the matching packaged exporter, and independently check freshness. No placeholder revision is supplied by these sources.

#### Measured export and execution sequence

Use the explicitly verified CLI as `$CAIRN`, the consumer root as `$ROOT`, and the absolute selected projection as `$POLICY`. `$PROJECTION_DRIVER` is a local Nickel expression applying `projection.ncl` to the reviewed inventory; `$INPUT_DRIVER` applies `inputs.ncl` to actual measured bindings. They are operator-owned inputs outside the observed source projection, not committed fake identity data. Use the reviewed exact Nickel evaluator to export:

```text
nickel export --format json "$PROJECTION_DRIVER" --field source --output target/cairn-completion-inputs/projection.json
nickel export --format json "$PROJECTION_DRIVER" --field dependencies --output target/cairn-completion-inputs/dependencies.json
"$CAIRN" check-evidence source --root "$ROOT" --projection target/cairn-completion-inputs/projection.json
"$CAIRN" check-evidence dependencies --root "$ROOT" --projection target/cairn-completion-inputs/dependencies.json
```

Take `subject_hash`/`source_projection_hash` and the sorted `dependency_hashes` directly from those Rust observations. Use Cairn's original `canonical_json_hash` for the selected parsed policy and exported registry; use `authoritative_command_hash` on each exact full executable-plus-args vector and `authoritative_selection_hash` on the complete required-check rows. Bind the measured values as `subject_hash`, `source_projection_hash`, `policy_hash`, `registry_hash`, `selection_hash`, `command_hashes`, `dependency_hashes`, `cargo` and `nix` in the input function. Do not substitute raw policy-file bytes for its canonical JSON identity, silently sort a changed check set, reuse old subject identities, or implement a second hash convention.

```text
nickel export --format json "$INPUT_DRIVER" --field contract --output target/cairn-completion-inputs/contract.json
nickel export --format json "$INPUT_DRIVER" --field profiles --output target/cairn-completion-inputs/profiles.json
nickel export --format json "$INPUT_DRIVER" --field applicability --output target/cairn-completion-inputs/applicability.json
"$CAIRN" check-evidence plan --root "$ROOT" --policy "$POLICY" --contract target/cairn-completion-inputs/contract.json --profiles target/cairn-completion-inputs/profiles.json --applicability target/cairn-completion-inputs/applicability.json
```

Review the returned exact plan and its hash. With explicit execution authority, create a new empty `target/cairn-completion` output directory and the root-relative one-byte `0` cancellation control `target/cairn-completion-cancel`. Measured inputs and cancellation controls stay under the already-ignored `target/` tree; they must not dirty the clean release source. `$PLAN_HASH` and `$ATTEMPT_BLAKE3` are the reviewed plan identity and fresh real BLAKE3 attempt identity, not literal tokens to put into evidence. Then execute once:

```text
"$CAIRN" check-evidence run --execute --root "$ROOT" --policy "$POLICY" --contract target/cairn-completion-inputs/contract.json --profiles target/cairn-completion-inputs/profiles.json --applicability target/cairn-completion-inputs/applicability.json --projection target/cairn-completion-inputs/projection.json --dependencies target/cairn-completion-inputs/dependencies.json --plan-hash "$PLAN_HASH" --attempt "$ATTEMPT_BLAKE3" --cancel-file target/cairn-completion-cancel --out target/cairn-completion
"$CAIRN" check-evidence verify --root "$ROOT" --policy "$POLICY" --contract target/cairn-completion-inputs/contract.json --profiles target/cairn-completion-inputs/profiles.json --applicability target/cairn-completion-inputs/applicability.json --projection target/cairn-completion-inputs/projection.json --dependencies target/cairn-completion-inputs/dependencies.json --plan-hash "$PLAN_HASH" --attempt "$ATTEMPT_BLAKE3" --out target/cairn-completion
"$CAIRN" agent-receipt complete --policy "$POLICY" --contract target/cairn-completion-inputs/contract.json --results target/cairn-completion/results.json --subject-hash "$SUBJECT_HASH"
"$CAIRN" agent-receipt smoke-mcp --root "$ROOT" --policy "$POLICY"
```

Require admitted complete publication, all four genuine result rows, passing original completion and MCP JSON verdicts, and no ignored dependency/command identities. Keep failed/timeout attempts intact and nonpassing. Read-only MCP executes no checks; process exit zero is insufficient. Parent must also run real failed-command/drift controls and invalid-present-selection rejection with the same selected policy (no fallback), then strict native validation, native coverage, policy freshness and full readiness. Require every original check's actual passing report fields; do not archive this change or mark runtime evidence complete from source preparation.

#### Required independent release obligations

The completion contract covers the four portable external checks, not recursive readiness. Native strict validation, traceability, policy freshness and readiness remain required independent rails. The separate KVM commands remain `nix run .#kvm-release-matrix -- --root . --matrix contracts/kvm-release/matrix.json --out "$MATRIX_OUT" --expected-revision "$CLEAN_REVISION"` and the Rust-owned `check-kvm-release-matrix` receipt validation against that same matrix, source revision and output. Preserve the operational 14,400-second outer matrix bound and 300-second validator bound; require `release-eligible`, all seven required row outcomes and artifact identity admission, not process success. The matrix retains its release-binary/guest builds, exact SMP, serialized snapshot replay, malformed virtio survival, admitted drift and fresh-workload replay rows unchanged. Portable MCP completion cannot close historical retention/replay, unavailable payload or remaining product-evidence tasks.
