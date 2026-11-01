# Release preparation verification — 2026-10-29

## Boundary

Preparation started from clean `1caeda2334f1d93e53ce22afbfa012b05458f606`.
The working tree was then changed during preparation; these observations are not
a clean-candidate release certificate. No tag, release, upstream visibility
change, repository publication, lifecycle archive or production promotion was
performed. Source edits add requirement comments and replace the implicit Serde
default for `AssertionRecord.must_hit` with an explicit `None` helper following
the existing pattern. The Octet flake source locator also changes without a
revision or NAR change. Owner: ChaosControl maintainers. Consumer: Rust
workload authors and local KVM operators. Repeatable capability: existing native
and build gates plus the public-source prerequisites in the release runbook.

## Exercised native checks

The initial ambient `cairn release-readiness --root .` returned JSON
`valid=false`, `verdict=fail` despite process exit zero. Native validation found
one checkbox missing its known requirement reference. Native coverage reported
448/448 referenced requirements but ten dangling IDs. Missing MCP completion
inputs were an independent failure, not an absent implementation to paper over.

The retained verified CLI was selected explicitly at the local audit locator
`target/planted-proof/retained-cairn-cli/bin/cairn`; the selected policy was the
absolute repository path `cairn-policy/generated/cairn-policy.json`. The cohort
and portable integration procedure remain owned by `cairn-policy/UPSTREAM.md`.
The retained executable is not a public install dependency.

After the repairs, the following commands ran with that CLI and explicit policy:

- `validate --root . --policy "$POLICY" --strict`: `valid=true`, no issues.
- `traceability coverage --root . --policy "$POLICY"`: exit zero,
  `463/463 referenced`; no missing or dangling links.
- `policy export --source cairn-policy/default.ncl --output cairn-policy/generated/cairn-policy.json --check`:
  exit zero, `policy fresh`; no projection rewrite.
- `release-readiness --root . --policy "$POLICY"`: `valid=false`, `verdict=fail`.
  Validation and traceability pass. `mcp_agent_smoke` fails with missing-file
  evidence. The authenticated release profile remains optional and absent;
  `onix_release_ready` is not promoted.

The native-process task reference was repaired. Existing delta specifications
were synchronized through `cairn sync NAME --root . --execute` for
`add-native-process-record-replay-profile`,
`model-guest-storage-flush-failures`, and `annotate-next-witness-values` after
passing their tasks gates and reviewing synchronization preflights. Storage
implementation and existing positive/negative test anchors now carry the actual
requirement markers. The Molten candidate campaign and existing closeout tasks
remain unchecked; all three changes stay active. No marker was deleted to clear
coverage and no incomplete campaign was declared executed.

## Source transport proof

Anonymous GitHub API retrieval of Octet commit
`9c7ba87bef2934d2b7b144167e13c8d18eac8958` succeeded. The exact Trellis commit
endpoint for `46ab2d92b9cfd2cfc4e631a56f3e667ee7263685` returned HTTP 404.
The workspace catalog identifies that Trellis repository as the canonical source;
its public availability cannot be inferred from a developer checkout.

`nix flake update octet` completed successfully. Only the `octet` lock node
changed. Its revision is unchanged and its NAR hash remains
`sha256-WekhhPZd6ow+qhSJwd1xbCQXe+EIwuTSC8YtvRU+7co=`. The root input now uses
`github:OnixResearch/octet/9c7ba87bef2934d2b7b144167e13c8d18eac8958` rather than a
sibling checkout. This is source-identity/transport evidence for that input only,
not fresh-cache proof of the complete dependency graph.

## Portable baseline

Pueue task 26 ran:

```sh
nix develop --no-write-lock-file -c cargo test --workspace --all-targets --locked --no-fail-fast
```

The completed task's structured verdict is `Success`. The Nix development shell
reported Rust 1.98.0 and Clang 21.1.8. Kache reported a cache-index disk I/O error
and explicitly disabled caching; compilation and the workspace command succeeded.
This is ordinary workspace execution, not a genuine measured Cairn producer
completion record. The preparation tree changed during this command, so it is
not evidence for one immutable clean revision.

## Additional exercised checks

Pueue task 27 passed the following commands before reaching the full Nix check:

```sh
nix develop --no-write-lock-file -c cargo fmt --all -- --check
nix develop --no-write-lock-file -c cargo clippy --workspace --all-targets --locked -- -D warnings
```

The subsequent unchanged `nix flake check -L` evaluated the outputs and selected
236 checks, then failed in the host's remote build hook: its gRPC builder URI
produced a Nix current-load lock filename exceeding the filesystem limit, followed
by `unexpected EOF reading a line`. This is an infrastructure failure, not a
passing flake verdict or a guest finding. No host configuration was edited.

The local-only retry, `nix flake check -L --option builders ''` (pueue task 28),
advanced beyond that hook and exposed a genuine source-policy failure:
`tigerstyle-chaoscontrol-focused` reported `integration-failure`, 602 warnings and
one denied `explicit-defaults` error at `AssertionRecord.must_hit` in
`crates/chaoscontrol-fault/src/oracle.rs`. Its bare `#[serde(default)]` was replaced
with `default = "no_must_hit"` and a private helper returning `None`, matching the
neighboring optional fields. No checker, lint severity, scope or requirement was
weakened. Existing warnings were not relabeled as clean.

After that correction, pueue task 30 ran
`nix develop --no-write-lock-file -c cargo test --locked -p chaoscontrol-fault`
and `nix develop --no-write-lock-file -c cargo build --locked -p chaoscontrol-fault`.
A throwaway executable linked the resulting library and exercised persisted JSON
records through `serde_json::from_value::<AssertionRecord>` and `verdict()`:
absent/null `must_hit` remained `Unexercised`, `false` gave `Passed`, `true` gave
`Failed`, and a string value was rejected. Serialization omitted an absent
`must_hit`. The smoke printed its successful outcomes; its source and executable
were removed. This proves compatibility of those supplied record cases, not
arbitrary report validity or a release verdict.

The first matrix-preflight invocation exceeded its 120-second tool deadline while
compiling. Pueue task 29 then exercised the real CLI with:

```sh
nix develop --no-write-lock-file -c env RUSTC_WRAPPER= cargo run --quiet --locked \
  -p chaoscontrol-evidence --bin run-kvm-release-matrix -- \
  --root . --matrix contracts/kvm-release/matrix.json --dry-run
```

It emitted all seven rows for `required-kvm-release-v1`, matrix identity
`blake3:e9c5ff6dd088cc59159aaa057e78de5cb9cd73d0e220ea6c80e956db6a86d9e0`.
This is command-planning evidence only, not execution of the seven KVM rows.
Kache diagnostics still appeared; no successful cache repair is claimed.

A throwaway local-link check exercised 55 Markdown file links across the changed
public guidance and preparation record with zero missing targets. The preparation
proposal/design/tasks gates passed. No permanent prose-copy tests were added.

## Retention and non-claims

Local observations are under `target/release-preparation-20261029/`; pueue owns
raw long-command logs. Those local locators are diagnostic, not public evidence
publication. Historical release, MCP and planted-finding receipts are unchanged.
No new VM campaign, assertion coverage, reproduced finding, source recovery,
public dependency publication, clean-candidate matrix or production verdict is
inferred from the lifecycle and documentation repairs.

## Measured preparation input identities

`b3sum` measured these files after the source-policy correction. These are byte
identities for selected inputs, not a whole-source release identity:

| Input | BLAKE3 |
| --- | --- |
| `flake.nix` | `26f0efb96c971ec90720343f9fc1bb73cd3e3db633c2c1db7e5a31470df52579` |
| `flake.lock` | `96af936e0ce78b3a43c796269ca400f02b6061d0254ed8943ced789713e3cc13` |
| `Cargo.lock` | `593d28484881d19950d2f29834f2b939f9114b6872996b024357d71bf43b45d4` |
| `crates/chaoscontrol-fault/src/oracle.rs` | `e4d45ebbe7795ae34e294ad81e3ebda60ddfa309a89c5dfc5e9a558773637a81` |
| `cairn-policy/generated/cairn-policy.json` | `07fbd4c7ba467f9b47806cd2510a255775d994ce1a9a896dbe5e1c8dab2740e6` |
| `contracts/kvm-release/matrix.json` | `4f1254e7150d4f31bef7be184c1f716f1aa1d84bc1c3f642d62bab2fd1917b3e` |
| `crates/chaoscontrol-evidence/src/assertion_summary_identity.rs` | `907c3024511d9e817511ab25ba54c618eb0b3f620570dbaae59292e935fca07e` |
| `crates/chaoscontrol-evidence/src/sdk_local_quality.rs` | `75cec104fae1321d7ea9a7dacc7fcaf6c282be88b78e2728b639e3728a5a8b55` |
| `crates/chaoscontrol-evidence/src/rust_automation/storage_campaign.rs` | `74e284e32f38245ebb5d6c30a311ce53d661c4b6873a9761ca987b5e980622ee` |

## Evidence default correction and consumer smoke

The follow-on full local check (pueue task 31) reached three additional denied
implicit defaults in the evidence crate after the oracle compiled. The focused
Octet report was `integration-failure`, with 2,283 warnings and three errors.
The corrections preserve `None` for summary and local-quality `must_hit`, preserve
the summary's non-null decoder, and preserve an empty vector for omitted storage
campaign `writes`. Each uses a private explicit helper; no shared abstraction,
lint exception, schema change or admission relaxation was introduced.

Pueue task 32 passed `nix develop --no-write-lock-file -c cargo test --locked -p
chaoscontrol-evidence`, including the existing must-hit/legacy-summary and SDK
local identity/verdict regressions, then built the library and both smoke CLIs.

Actual consumer execution after the corrections:

- `check-sdk-assertion-quality` ran its existing positive/negative fixtures and
  printed `sdk-assertion-quality: ok`, exit zero.
- `storage-recovery-campaign --input .cairn/changes/model-guest-storage-flush-failures/evidence/storage-campaign-cli/cc-campaign-pass.json --out target/release-preparation-20261029/storage-legacy-receipt.json`
  admitted the retained request with omitted `writes`, exited zero and returned
  `verdict=Pass`, receipt `blake3:e09d21dabad3e7822a9f3467b6a7e005514821f109d0864e80c1d92d8fdea620`.
  A copy with `writes=null` exited 2 with an invalid-sequence error and created no
  receipt. These are pure storage-model observations, not guest/KVM execution.
- A temporary Cargo example called the public assertion-summary APIs: omitted
  metadata remained admitted but non-promoting, null/string metadata was rejected,
  and explicit Boolean verdicts remained accepted. The first standalone rustc
  invocation selected a mismatched serde-json artifact and did not run; using
  Cargo's dependency graph produced the successful smoke. Temporary sources were
  removed; no implementation-default or prose-copy tests were made permanent.

The later native validation still passes; traceability remains 463/463. Native
readiness still fails on missing current completion inputs, not those repaired
linkage checks.

## SDK documentation correction

Pueue task 33 passed formatting and denied-warning Clippy after all four default
corrections, then the full local Nix check failed in `checks.x86_64-linux.rustdoc`.
Rustdoc denied the unresolved `cc_assert_always_any!` link in the public
`cc_assert_always_some!` documentation. The link now names the crate-root export,
`crate::cc_assert_always_any!`; no macro body or API changed. The corrected
`crates/chaoscontrol-sdk/src/assert.rs` measured BLAKE3 is
`be264d41fb238bf14d83954740fb7265d9ace0732604a6f6eb63225b44275e80`.

The following full local run used `--keep-going` so independent check failures
would not cancel remaining evidence collection. It retained the same flake check
set and disabled only the broken host remote-builder hook for this invocation;
no host configuration or repository gate was weakened.

## Clean-candidate full check and KVM attribution

Commit `3db8a19c` was evaluated that way. The run listed 106 checks and failed
four attributes:

| Check | Failure | Status after repair |
| --- | --- | --- |
| `evidence-contracts` | `accepted-dogfood-config` I/O error per workload | Fixed; passes on `fa6f3256` |
| `replay-readiness` | Same `accepted-dogfood-config` I/O error | Fixed; passes on `fa6f3256` |
| `snapshot-replay-smoke` | `HostWatchdogTimeout { vcpu: 0 }` | Fails in isolation; base revision also fails |
| `local-multi-hypervisor-kvm-smoke` | Raft 240 s dogfood timeout; six Rust-workload `HostWatchdogTimeout` attempts | Fails in isolation; documented blocker class |

The shared cause of the first two was `repository_root_for_manifest` deriving an
empty root from the default relative manifest
`dogfood-results/accepted-workload-proofs.json`, which cannot be canonicalized.
The root now falls back to the current directory;
`relative_manifest_paths_resolve_the_current_directory` covers relative, bare, and
absolute paths. The real CLI then reported
`accepted-dogfood-config: 4 workload configs match; 0 historical identity artifact(s) are diagnostic-only and non-promoting`
with exit zero, and
`nix build .#checks.x86_64-linux.evidence-contracts .#checks.x86_64-linux.replay-readiness`
succeeded on `fa6f3256`.

KVM attribution used the same check on base `1caeda23` in an isolated worktree:
`snapshot-replay-smoke` failed with `rc=124` and
`missing checkpoint.json after run`. The candidate failed in isolation with
`Exploration failed: Vm { source: HostWatchdogTimeout { vcpu: 0 } }`. The
multi-hypervisor smoke, run explicitly with `--out`, retained one Raft attempt
ending in `accepted-snapshot-verdict-dogfood timeout after 240s` and six
Rust-workload attempts ending in `HostWatchdogTimeout { vcpu: 0 }`, with campaign
status `failed` and zero passed runs. These are operational timeouts, not
reproduced guest findings; both rails are retained as failures and are not
relabeled. One earlier multi-hypervisor artifact was discarded because an
accidental `--help` argument was forwarded into the nested runs; it is not used
as evidence.

The new active preparation change required a product-scope intent. The reviewed
intent was added to `contracts/product-scope/registry.ncl` with a `blocked` target
state and non-claims, the JSON projection was regenerated from the Nickel source,
and `check-product-scope --root . --write` refreshed the owned scope documents and
passed. The generated `docs/product-scope-status.md` fact block and
`contracts/product-scope/generated/product-scope.json` now include that active
change. The scope projection does not promote the change; it records intent only.

The repository-owned license checker was also compiled with the selected Nix
development toolchain and executed against the working tree:

```sh
nix develop --no-write-lock-file -c rustc tools/check-license-boundary.rs \
  -o target/release-preparation-20261029/check-license-boundary
target/release-preparation-20261029/check-license-boundary .
```

It printed `license boundary valid: 62 rules` and exited zero. This is conformance
to that checker and policy, not legal advice or a public-source availability
verdict.
