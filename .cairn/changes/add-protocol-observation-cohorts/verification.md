# Protocol-observation verification

## Status

The change remains active and unarchived.
The user requested an unfinished checkpoint commit and branch push on 2026-09-05.
Checkpoint `c50e01cfca4b9e441587a570bd7ed9fc37bdb558` is on the change branch at `origin`.
Two tasks remain open. Stored-parent replay now passes its explicit KVM tests.
The checkpoint does not authorize lifecycle completion or integration into `main`.
Follow-up details are in `evidence/verification-follow-up-2026-09-05/README.md`.
The later Cargo and source repairs use `evidence/gate-repair/`.
The current source pass adds `7ed930b`, `4959d75`, and `4343e37` after the published `61097cd` checkpoint.
The retained `source-checked-*` logs describe the current product inputs.
They do not establish strict acceptance or accepted-contract publication.

The worktree starts at `31300fa1a2d29c7496e8316f065c156f80343143`.
The branch is `drain/protocol-observation-cohorts-20260904`.
The dirty primary checkout and Mantle remain unchanged by this work.

## Checked implementation

- Typed Nickel profiles bind finite limits, schemas, producers, participants, generations, oracle authority, novelty selection, and non-claims.
- The pure core admits opaque records and derives domain-separated BLAKE3 identities.
- Cohorts retain canonical source journals, host losses, classifications, and full novelty identities.
- The SDK checks counters and framing before transport effects.
- The VMM supplies the VM identity, executing vCPU, exit sequence, and scheduler identity.
- The independent Raft fixture ignores runtime pass fields and detects conflicting leaders at its declared term boundary.
- Receipts revalidate cohort, oracle, marker, and context bindings.
- Fault-run resets preserve the protocol journal and loss count. An admitted snapshot restore can replace that execution history.
- Marker status reports `identity-linked`, not snapshot reachability.

## Baselines and negative controls

The initial relevant package suites passed before core implementation.
The original baseline log is `/tmp/chaoscontrol-protocol-baseline-tests.log`.

`evidence/attempts/adversarial-baseline.log` retains the first accounting counterexamples.
They cover excessive limits, changed duplicates, post-final records, and unsupported projections that hid malformed input.

`evidence/attempts/reset-status-before.log` retains two later failing controls.
A fault-run reset erased a recorded host loss.
A pure marker binding produced the unsupported status `reachable`.
Both controls pass after the corrections.

The no-default SDK build initially failed because the guest-supervisor binary lacked its `full` feature requirement.
`evidence/attempts/sdk-no-default-before-bin-gate.log` retains that failure.
The corrected checkpoint build succeeds with two inherited coverage-helper warnings.
The later feature-aware test repair also removes those warnings from the minimal library build.
Its all-target no-default tests pass 15 protocol and 42 SDK cases. Strict Clippy passes in the same scope.
Full-only example, supervisor, catalog, and multiprocess targets retain explicit feature requirements.
The three new target requirements reject explicit no-default requests. Their full-feature tests remain enabled.
`evidence/gate-repair/minimal.md` records the fresh failures, two target corrections, and positive and negative outcomes.

## Checkpoint validation

The checkpoint logs use `evidence/verification-2026-09-05/`.
The table describes that source checkpoint, not every later edit.

| Check | Result | Evidence |
| --- | --- | --- |
| Six relevant packages, all targets and all features | Passed | `all-target-tests.log` |
| Strict six-package Clippy, all targets and all features | Passed | `clippy.log` |
| Protocol-observation integration cases | 23 passed within the package suites | `all-target-tests.log` |
| No-default protocol and SDK build | Passed with two inherited warnings | `no-default-features.log` |
| License boundary | Passed, 59 rules | `license-boundary.log` |
| Final scoped Nix tests and contracts | Passed, 23 cases and seven invalid profiles | `nix-focused.log` |
| Final KVM shared-page snapshot case | Passed, one explicit ignored-test invocation | `kvm-page.log` |
| Changed Rust sources and Nix formatting | Passed | `rustfmt.log` and `nixfmt.log` |
| Final Octet check | Blocked by two inherited guest-probe errors | `octet.log` |
| Product scope | Blocked by the missing `adopt-campaign-core` intent | `product-scope.log` |
| Contract registry | Passed | `registry.log` |
| Lifecycle validation and three gates | Passed | `cairn-validate.json`, `cairn-proposal.json`, `cairn-design.json`, and `cairn-tasks.json` |
| Full flake check | Blocked by the Cargo metadata panic in `dependency-policy` | `flake.log` |

The initial scoped Nix check passed 18 integration cases before the later regression fixtures.
The Nickel check admitted the exact exported profile and rejected all seven invalid profiles.
Earlier KVM evidence covers host dispatch and snapshot retention, not an executed protocol-aware guest continuation.

## Build repairs

Crane already adds all-target scope to its dependency check.
The focused dependency arguments no longer duplicate that flag or request a named integration test from the dummy source.
The actual Nix test still runs the named protocol-observation suites.

Optional Serde fields retain missing-field compatibility without implicit external defaults.
The existing process queue and process-identity set use explicit empty constructors.
Positive and negative compatibility fixtures cover omitted fields, wrong types, and missing required fields.
The license policy now includes the existing guest-determinism probe with its existing workspace license.
No license grant or third-party notice changed.

## Remaining boundaries

Four explicit KVM tests now cover `Session::replay` through the production controller and file store.
The guest copies an SDK-produced frame, emits port I/O, and increments a memory counter.
A bounded slice stops before HLT. Changed counters prove that replay restores and resumes the guest instead of reusing a stale journal.
Negative cases cover bindings, bounds, missing or corrupted snapshots, incomplete observations, and malformed ELF input.
This result does not establish Linux boot, in-guest SDK initialization, or general halt behavior.

The checkpoint Octet check rejected two implicit guest-probe defaults.
Both probe paths now use one fully explicit constructor for the reviewed CPU, memory, boot, and scheduler configuration.
The seed retains its full width. Four probe tests pass, including an exact comparison with the previous configuration.
That follow-up Octet check reported zero errors and 2,458 warnings.
Later source corrections reduced the same pinned report to 1,455 warnings and zero errors.
The corrections cover explicit derives, qualified owner paths, and private protocol names with public compatibility exports.
The continued passes cover admitted inline Serde scopes and exact integer framing without substituted lengths.
The latest three-batch pass removes 307 findings and repairs the separate minimal test harness.
The current aggregate reduction is 1,003 findings from the original 2,458-warning report.
Seven-package tests and strict Clippy pass across all targets and all features after these corrections.
Naming, owner-path, module-filename, and generated-source findings remain.
The warning-only result is not strict acceptance evidence. The quality task remains open.
No lint catalog, scope, warning budget, baseline, or severity changed.
`evidence/gate-repair/` retains each source batch and the failed controls.

The product-scope guard now passes after review of ten missing intents against their existing proposals.
The registry retains all capability states and records the publication and parity prerequisites for those plans.
The guard itself remains unchanged. Generated documents now match the repository facts.

The checkpoint flake check failed in `checks.x86_64-linux.dependency-policy`.
Unpatched Cargo still panics during pathless Radicle package-ID formatting.
The trailing-slash workaround remains invalid because `git-remote-rad` rejects that namespace.
The repository now supplies an explicit Cargo compatibility package with formatter and parser corrections.
All 14 schema tests, actual locked offline metadata, and the VM Cohort package-ID check pass.
The dependency-policy Nix check passes after the policy records three existing adoption and license facts.
Near-match source and license controls retain their expected rejection.
The VM Cohort URL and revision remain unchanged. The normal compiler and host Cargo installation remain unchanged.
`docs/cargo-radicle-compatibility.md` describes the provider and its bounded claim.

The broad flake retries reached HTTP 403 failures for four Wasm crate downloads.
The official static mirror supplied their exact expected fixed-output objects without a dependency or lockfile change.
The logs and GC roots retain these transport repairs.
Later checks exposed and resolved isolated adapter lockfile drift and the same Cargo panic during artifact installation.
Cargo regenerated only two local `serde_json` dependency edges in the isolated adapter lockfile.
Its unchanged strict Octet gate passes with zero findings and no lockfile mutation.
The current `source-checked-nix.log` repeats this isolated result alongside the broader 1,455-warning report.
Both Crane compositions now use patched metadata for artifact selection while retaining the normal Cargo build command.
The Raft guest build and the missing-command negative control pass their expected outcomes.

The earlier broad check reached a separate VM Cohort vendor-layout error.
The vendored `vm-cohort-conformance` crate did not resolve `../../../config/generated/profile.json` from `src/standard.rs:33`.
`evidence/gate-repair/flake-final.log` retains that failure.
The private vendor workspace now preserves the original Rust files and exact profile bytes behind a package symlink.
Compiler parity, negative controls, nine real adapter cases, and dependency policy pass.
Both host and musl builds consume the same explicit vendor projection.
The source-filter controls reject `.cairn/` and `.pi/` inputs without excluding similar directory names.

The current broad retry rejects the SpaceWasm bundle manifest before runtime comparison.
`source-checked-flake.log` reports the same `ded66a4959c9efeda62f2eb3d13a06de6df0ad01a1d53f222c199ab6e66d9eb7` digest as the retained rooted manifest.
The admitted digest remains `13058ea2d9913348a203cceff7b58d98b6446610ac80518dc3359b8d7ee57472`.
The rooted bundle also differs from the admitted bundle identity, although its host runner digest matches.
`evidence/gate-repair/spacewasm.md` records the exact digests and the missing evidence for a cause diagnosis.
No producer worktree, provider pin, expected digest, or admission guard changed.
A release-wide validation pass remains unavailable.

Task 559 passed the scoped Nix checks, KVM case, and formatting.
Task 560 retained the Octet failure. Task 573 retained the product-scope blocker after projection export.
Task 574 retained the full-flake failure. Task 538 passed lifecycle validation and all three gates.
`product-inputs.b3` and `build-inputs.b3` bind the checked product files and build inputs.

Tasks 449 and 454 did not run their checks because `/tmp` rejected log creation with `No space left on device`.
Task 444 stopped during Nix garbage collection without usable checker output.
Task 455 failed with exit 102 after the lifecycle task file changed during its delayed evaluation.
That attempt is invalid validation evidence. The next attempt uses unchanged inputs throughout evaluation.
Private-home and `/var/tmp` build-directory overrides also failed Nix security checks.
The retained attempts explain those failures. No home permission or Nix security rule changed.
The configured default build directory worked after the storage pressure cleared.
Final retries use that default and retain logs in this change package.
Neither the storage failures nor the changed-input attempt are validation passes.

## Current validation

The seven-package all-target, all-feature tests, strict Clippy, strict Rustdoc, and scoped formatting pass after the final target declaration.
The original no-default build, compatible no-default tests, and strict no-default Clippy also pass.
The focused Nix command passes protocol tests/contracts, vendor controls/adapter, dependency policy, source filtering, the license boundary, and strict isolated adapter checks.
Four KVM replay cases pass in 68.28 seconds under a 120-second external timeout after their build phase.
The replay claim remains limited to the declared pre-halt ELF profile.

The source pass uses all three declared batches. Its final source report remains warning-only.
The full flake check remains blocked by SpaceWasm manifest admission.
No accepted-spec sync, archive, or main integration occurs. Both remaining tasks stay open.

## Non-claims

Guest process references are declarations, not process authentication.
The oracle trait is a reviewed pure-code contract, not a plugin sandbox.
The consumer retains protocol semantics and oracle authority.
Snapshot links do not prove reachability, restorability, or guest continuation.
The evidence does not prove universal correctness, production readiness, release eligibility, a protocol total order, or synchronized wall clocks.
