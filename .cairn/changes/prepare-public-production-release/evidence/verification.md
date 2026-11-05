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

A later `nix flake check --no-build` on the final documentation commit failed
because the store path
`/nix/store/2xcwjdylcxvzcvnby1zjcl73rz1fdpk8-mantle-spacewasm-bundler-source.drv`
was no longer valid. The same spacewasm check passed in the earlier full run, and
no preparation commit touches that source; this is a garbage-collected input in
the local store, not a repository verdict. It is recorded rather than suppressed.

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

### Transport cutover (public acquisition edges)

The lock contained three `ssh://` transitive inputs and two `rad://` inputs. Root
input overrides now pin the same revisions through anonymous public endpoints:

- `trellis.inputs.octet` → `github:OnixResearch/octet/09b2d863…`
- `mantle.inputs.artifactAuthSource` → `github:OnixResearch/onix-artifact/c932138d…`
- `mantle.inputs.cairn.inputs.artifact` → `github:OnixResearch/onix-artifact/e41340be…`
- `campaign-src`, `choregraph-history-src` → `git+https://seed.radicle.garden/<id>.git`

After `nix flake lock`, the only changed nodes were those five, and each kept its
exact `rev` and `narHash`; no other node changed. `nix eval --raw
.#checks.x86_64-linux.fmt.drvPath` succeeded and that check built successfully with
the rewired inputs. The source-tree formatting gate also exposed that the new
guard regression test was not rustfmt-clean; `rustfmt` was applied and the gate
passes.

Two private boundaries remain: the root Trellis input (`file:../trellis`, anonymous
commit 404) and VM Cohort (`rad://` private, public seed 500). Neither can be made
public by this repository.

### Merge with remote main

Remote `main` advanced to `eb2eca64` (Molten provider contracts and protocol
observation). Merging it produced one conflict in
`.cairn/specs/smr-reliability-workload/spec.md`; both sides had appended different
requirements, so both blocks were kept (remote's stale-duplicate rollback
requirement first, then the storage-recovery requirements) with no requirement ID
duplicated.

Traceability immediately after the merge was `465/465` referenced with six
dangling IDs:
`chaoscontrol.protocol_observation.{cohort,envelope,evidence,novelty,oracle_boundary,profile}`.
Those markers existed in `crates/chaoscontrol-smr/src/observation*`, but their
spec was still only an active delta, so the IDs were not accepted. Remote `main`
alone reported sixteen dangling IDs with the same six among them; the merge
removed the other ten through the synchronized native-process, next-witness, and
storage requirements.

That gap is now closed by implementing the missing delta instead of narrowing it:

- `observation::marker::MarkerBinding` binds a declared marker to an admitted
  record, logical boundary, projection, cohort, and restorable parent snapshot,
  and retains (never upgrades) cohort completeness.
- `observation::oracle` publishes the consumer-owned `ProtocolOracle` boundary,
  typed `OracleResult`/`OracleVerdict`, and `evaluate_oracle`, which records an
  incomplete cohort without invoking the oracle and rejects a runtime self-oracle.
- `observation::receipt::ProtocolReceipt` binds profile, bounds, boundary, cohort,
  completeness, record, novelty, oracle, marker, snapshot, scheduler, fault,
  replay, and non-claim facts, and refuses identity drift, completeness upgrades,
  a pass on an incomplete cohort, and missing non-claims.
- The profile contract admits `marker_policy = "declared"`; `nickel typecheck`
  passes. New positive and negative tests cover marker binding, oracle self-report
  rejection, receipt binding, unknown participants, and novelty selection drift.

`cairn sync add-protocol-observation-cohorts --execute` then accepted the delta.
Traceability is `473/473` referenced with zero missing and zero dangling IDs, and
`validate --strict` is `valid=true`. The change keeps five open shell-adoption
tasks (SDK surface, VMM collection, explorer novelty guidance, downstream contract
publication, operator status); the pure requirements and their evidence are
complete.

`cargo fmt --all -- --check`, `cargo clippy -p chaoscontrol-smr --all-targets -D warnings`,
`cargo test --workspace --all-targets --locked --no-fail-fast` (230 suites), the
Octet consumer check (`warning-only`, zero errors), policy freshness, and product
scope pass on the merged tree.


### Watchdog retry and harness kill (after the merge)

Two operational defects were repaired:

- A runnable single-vCPU guest now retries across consecutive host watchdog
  intervals instead of failing on the first `SIGALRM`. Only an uninterrupted
  series with no intervening KVM exit (about five seconds at the 100 ms default,
  `CHAOSCONTROL_WATCHDOG_US`) remains `HostWatchdogTimeout`. Retries change no
  guest state, exit count, or virtual TSC. `cargo test -p chaoscontrol-vmm --lib`
  passes 569 tests with the new bound.
- `scripts/snapshot-replay-smoke.sh` now uses `timeout -k 10` and accepts the
  killed status, because a blocked vCPU can ignore `SIGTERM` and outlive the
  harness bound. This was observed directly: a raised-interval probe outlived its
  `timeout 360` for hours.

End-to-end verification of the snapshot rail now passes. With a GC-rooted
cached kernel (`d1gy1wcx…-chaoscontrol-vmlinux/vmlinux`) and a retained
repository-built raft initrd (`vb6wpgml…-chaoscontrol-initrd-raft`), the repository
smoke script completed in 387 seconds with exit zero:

```text
replay verdict artifact ok: run=replay-1793637619662 bug=2 assertion=3463273124 depth=1
snapshot replay smoke ok: bug_2.json
```

The retained verdict records `replay_class=snapshot_backed_reproduced`,
`reproduced=true`, `replay_parent_depth=1`. Before the retry change the same
kernel/initrd combination failed with `HostWatchdogTimeout { vcpu: 0 }` (and the
base revision failed with `rc=124`). The official `.#checks.x86_64-linux.snapshot-replay-smoke`
was not re-run because rebuilding the current guest artifacts stayed queued
behind unrelated daemon work; the manual run uses a retained initrd from the same
repository lineage rather than the current derivation. The multi-hypervisor rail
was not re-run for the same reason. Both remain required before any release claim.

### Cargo acquisition and build contention

An empty-`CARGO_HOME` `cargo fetch --locked` in the development shell completed in
six seconds, populating 504 crate archives and every locked Git source, including
`rad://` Campaign/VM Cohort and `git.onix.computer` bounded-exec. This host has
`git-remote-rad` installed, so it does not prove anonymous public acquisition; a
host without that helper or private-repository access still fails. See the
release runbook for the owner decision.

End-to-end KVM verification remained blocked by the shared host. The Nix daemon
was saturated by unrelated builds (one foreign job held most CPUs), so the
kernel/initrd build for the rails stayed queued for over eight hours without
starting work and was stopped. A second queue daemon had lost its configuration
and state. No rail result is inferred from this condition; the retry semantics and
harness kill remain covered by unit tests only.

### Current-initrd catalog conflict (fixed)

The official `snapshot-replay-smoke` check with the current `initrd-raft`
derivation initially failed snapshot validation:

```text
invalid VM 0 assertion snapshot: Status; catalog=FatalConflict accepted_catalog=false
legacy=0 structured=0 conflicts=64
conflict_samples=[NoActiveRun, AlreadyBegun, CatalogIncomplete, CatalogIncomplete]
```

The cause was the SDK's run-independent metadata event (`antithesis_sdk`)
arriving during first-use initialization before any run was active. The oracle
treated it as a bound event, marked `NoActiveRun` as a fatal identity conflict,
and then rejected catalog admission as `AlreadyBegun`/`CatalogIncomplete`.
`PropertyOracle::record_event` now admits `SDK_METADATA_EVENT` independent of run
scope, and `sdk_metadata_before_a_run_does_not_poison_identity` covers the
positive and negative behavior; a later unknown event without a run still marks a
conflict.

With that fix the repository smoke script passed with the current initrd in 355
seconds and validated a `snapshot_backed_reproduced` verdict at parent depth 1.
The official check then passed with its out-link:

```text
snapshot-replay-smoke-check> replay verdict artifact ok: run=replay-1793679699376 bug=1 assertion=3463273124 depth=1
snapshot-replay-smoke-check> snapshot replay smoke ok: bug_1.json
```

The multi-hypervisor check then passed with its own kernel variant and out-link:

```text
local-multi-hypervisor-kvm-smoke-check> wrote .../1/campaign-receipt.json
  (replay-readiness-local-multi-hypervisor-campaign status=recorded
   campaign=local-kvm-smoke-0001 hypervisors=2 runs=2 passed=2
   restart_persisted=true workloads=raft,rust-workload
   fault_classes=block,network,scheduler,timer scope=bounded-local-multi-hypervisor)
local-multi-hypervisor-kvm-smoke-check> local multi-hypervisor KVM smoke artifacts: .../1
```

Both KVM rails are green on the current revision. The seven-row release matrix
has not yet been run on this revision.

### Seven-row release matrix (partial)

`nix run .#kvm-release-matrix` ran on clean `f03aa793` with local builders. Five
rows passed before the artifact-policy repair; after allowing read-only
`/nix/store` out-link targets, both build rows retained their artifacts and
passed, so six of seven rows pass:

- passed: `build-release-binaries`, `build-raft-guest`, `deterministic-smp`,
  `serialized-snapshot-replay`, `virtio-malformed-input`, `admitted-drift`
- failed: `fresh-workload-replay`

The fresh row ran `raft-accepted-verdict-dogfood --max-attempts 1 --start-seed 0`
with probe `fail_after=1` and failed during exploration with
`fault outcome transition failed: ApplicablePlanMismatch`. Instrumenting the
transition showed the selected attempt and planned effect agreed on every value
but had no match arm:

```text
fault=DiskFlushError { target: 0, delay_ns: 9535677 }
effect=BlockFlushError { target: 0, delay_ns: 9535677 }
```

`plan_effect_matches_attempt` had no `DiskFlushError`/`BlockFlushError` arm, so any
randomly selected flush error aborted the run. The arm is now present with a
positive and negative regression test
(`flush_error_plan_binds_its_selected_attempt`), and direct exploration with the
same seed/probe completes and writes a checkpoint instead of failing.

A full matrix rerun on the clean fix revision was not completed: Nix auto-GC
blocked on another process's big-GC lock, and the subsequent local-builder run
stayed queued behind unrelated daemon builds for over 40 minutes without creating
its output directory. The six previously passing rows remain the current matrix
evidence; the fresh row needs a rerun on a quiet host.

On `aa44dcf2`, the three rows whose commands do not write the Nix store were
rerun directly with the contract commands and passed:

```text
cargo test -q -p chaoscontrol-vmm --test deterministic_smp_kvm     3 passed
cargo test -q -p chaoscontrol-vmm --test exact_snapshot_kvm        1 passed
cargo test -q -p chaoscontrol-vmm --test virtio_kvm_smoke -- --ignored  1 passed
```

`/dev/kvm` was present and each run opened a real KVM vcpu. This is row-level
command evidence, not a matrix receipt: the executor (`run-kvm-release-matrix`)
still cannot complete because `build-release-binaries`, `build-raft-guest`,
`admitted-drift`, and `fresh-workload-replay` require store writes. At the time of
this check, every store write stalled in the daemon: `nix eval .#default.outPath`,
`nix build .#default --dry-run`, and `nix build nixpkgs#hello --no-link` all blocked
with `waiting for the big garbage collector lock...`, while read-only evaluation
(`nix eval .#default.name`, `nix eval nixpkgs#hello.name`) returned immediately.
The store had 91–96 GiB free against `min-free = 100 GiB`, and the client blocked in
`unix_stream_read_generic` on the daemon socket. Clearing that lock
needs the operator's root access to the Nix daemon.

The `fresh-workload-replay` command was reproduced directly on `aa44dcf2` with the
debug `accepted-snapshot-verdict-dogfood` and `chaoscontrol-explore` builds, the
admitted raft cohort bounds, and the previously built vmlinux and initrd-raft store
artifacts:

```text
--cohort contracts/fresh-workload-proofs/cohort.json --max-attempts 1 --start-seed 0
--run-timeout 240 --export-timeout 300 --repro-timeout 300 --vms 3 --rounds 3
--branches 2 --ticks 80 --memory-mb 256 --fail-after-values 1 --assertion-id 3463273124
```

The attempt completed exploration, wrote a checkpoint, and exported `bug_1.json`
with `has_assertion_identity: true`, `has_snapshot_ref: true`, and
`replay_parent_depth: 1`. No `ApplicablePlanMismatch` occurred, so the plan-binding
abort is gone. The attempt is not accepted: `run_exit_status` and
`export_exit_status` were `124` because the unoptimized debug binaries exceeded the
contract's 240 s and 300 s bounds. That is a binary-profile effect, not bound
relaxation; row acceptance still needs the release binary the store-dependent row
would build.

Passing artifact paths through out-link symlinks fails earlier with
`Os { code: 40, kind: FilesystemLoop }`; use resolved store paths.

Repeating the same command with `cargo build --locked --release` binaries and the
same bounds accepted the replay:

```json
{"accepted": true, "reproduced": true, "replay_class": "snapshot_backed_reproduced",
 "schema_version": 2, "replay_parent_depth": 1, "assertion_id": 3463273124}
```

The accepted bug is `bug_1.json` for the cohort's `snapshot-replay-probe.v1`
assertion, with assertion identity and a snapshot reference; the other 75 exported
findings were not selected for acceptance. The verdict is
`target/release-preparation-20261029/fresh-row-release/fresh/replay-verdict-bug1.json`.
These binaries are plain Cargo release-profile builds, not the flake `chaoscontrol`
package the matrix row resolves, so this proves the row command and the fixed
fault-plan binding under the admitted bounds — not a matrix receipt.

### Watchdog timing probe (diagnostic only)

Two additional bounded probes used the same kernel, initrd, and arguments with the
debug `chaoscontrol-explore` binary. Their purpose was attribution, not a verdict.

- Default watchdog, outer bound `timeout -k 5 180`: the run was still making
  progress at Raft tick `15600` when the outer bound fired (`rc=124`). No
  `HostWatchdogTimeout` appeared, so the same workload needs more wall time than
  that bound.
- `CHAOSCONTROL_WATCHDOG_US=5000000` with the repository smoke script: the run
  stalled after guest serial line `Serial: 8250/16550 driver` and did not complete;
  the script's own `timeout 360` did not terminate it after several hours, so the
  orchestrator was killed and the orphaned VMM processes were force-terminated.
  This is an unfinished diagnostic, not a pass or a guest finding.

These observations show the required KVM rails are wall-clock sensitive: the
single-vCPU 100 ms no-exit watchdog can interrupt a runnable vCPU and become a
permanent `HostWatchdogTimeout`, and the enclosing script bound did not reliably
terminate the stalled process. Fixing that is a design decision about operational
versus evidence bounds, so no bound, signal semantic, or check was changed here.

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
