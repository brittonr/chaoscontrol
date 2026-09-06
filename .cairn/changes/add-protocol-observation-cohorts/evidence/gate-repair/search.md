# Gate repair search

## Completion contract

Repair Cargo/Radicle compatibility and the reported Octet findings without suppressions or weaker gates.
Keep VM Cohort at `ab123e3673b6dd616b3df5d044026b5e85755149`.
Preserve protocol bytes, snapshot compatibility, authority boundaries, and negative controls.
The retained worktree starts at `c9e93760b0be1e1cd2c737ba3ea90a54db16034b`.
The primary checkout and other change worktrees remain outside this repair.

Completion requires valid locked Cargo metadata, the dependency-policy Nix check, and a strict Octet result over the declared scope.
Affected tests must pass before and after source changes.
A warning-only result, a bypassed dependency check, or a smaller lint scope cannot establish completion.
Full lifecycle completion also requires the remaining publication task and normal Cairn closeout.

## Search limits

This session uses serial, correlated review lenses. It does not use subagents.
Start with three Cargo mechanisms and four focused diagnostic rounds.
For Octet, classify the existing report before source corrections. Use bounded correction batches and rerun the compiler after each batch.
Reopen a rejected mechanism only with new evidence. Record any added search budget and its reason.
Use repository sources and pinned provider contracts. Do not read secrets or change host services.
Do not change `flake.lock` except through Nix.
Allowed terminal states are validated, blocked, exhausted, and user-decision-required.

## Approach registry

| Family | Mechanism | Claim | Artifact | Evidence | Gap strength | Blocker | Next check | State |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Radicle namespace | Use the delegate namespace as a URL path | Exact source retrieval retains the revision | Namespace metadata control | Refs exist, but Cargo requires the absent `HEAD` | simpler | The transport lacks the required ref | No retry without a changed transport contract | falsified |
| Cargo formatter and parser | Repair pathless package identity handling | Locked metadata and dependency policy retain the exact source | `nix/cargo-radicle.nix`, two patches | `cargo.md`, commit `c822499` | simpler | No remaining blocker in this scope | Preserve the provider and negative controls in final checks | validated |
| Published seed transport | Use a published seed URL | Hierarchical transport supplies the exact source | Seed probes in `cargo.md` | Garden returned 500 and the Onix seed returned not found | unknown | Neither route supplied the dependency | No retry without new publication evidence | blocked |
| Qualified owner paths | Qualify reported references without changing values | Hidden-owner findings decrease | Source batches and bounded AST helpers | Seven-package tests, strict Clippy, and scoped Nix checks pass | simpler | Broader source findings remain | Select the next reviewed source family without bypasses | validated |
| Module vocabulary | Use local names behind existing public re-exports | Private names avoid repeated ownership words | Protocol source and explicit compatibility exports | Seven-package tests and strict Clippy pass | simpler | Broader naming debt remains | Retain public paths and wire-field checks | validated |
| Exact crate mirror | Retrieve identical bytes through the official static mirror | A transport repair retains the fixed-output identity | Two rooted Nix objects | `wasm-smith-mirror.log`, `wasmparser-mirror.log` | simpler | Full-flake completion remains unproven | Rerun the broad check on stable inputs | validated |
| Isolated adapter lock | Let Cargo record the current local dependency edges | The strict adapter gate preserves its input lockfile | Generated isolated `Cargo.lock` | Two edges added, zero source findings, lock guard passes | simpler | No remaining blocker in this scope | Preserve the guard in broad checks | validated |
| Artifact metadata | Route host and musl install metadata through the patched Cargo | Artifact selection retains workspace authority and the original build compiler | Shared Crane hook wrapper | Raft build and missing-command control pass | simpler | The broad check still needs a final run | Run the broad check on stable inputs | validated |
| Vendor resource layout | Retain a private workspace behind the package symlink | Conformance compiles without source or profile changes | `nix/vm-cohort-vendor.nix` | `vendor-final.log` passes controls, nine adapter cases, and policy | simpler | No remaining blocker in this scope | Retain the exact-resource controls | validated |
| Serde inline scopes | Qualify admitted derives and remove compiler-proven unused imports | Derive ownership stays explicit without wire changes | `serde-scopes.md` | Tests and Clippy pass, report decreases to 1,766 | simpler | Ambiguous scopes remain outside the helper | Retain the rejected scopes for manual review | validated |
| Exact integer framing | Assert the target-width invariant instead of substituting lengths | Representable canonical bytes stay exact | `framing.md` | Tests and Clippy pass, report decreases to 1,762 | simpler | Other integer policies need separate admission review | Retain boundary regressions | validated |
| Producer bundle identity | Remeasure the declared SpaceWasm bundle | Only the admitted complete bundle can execute | `spacewasm.md` and observed manifest | Both observed manifest digests differ from the admitted digest | equivalent | Compatible producer evidence or a new-cohort review is absent | Resolve the producer cohort without changing the guard | blocked |
| Remaining source rules | Correct each rule at its owner | The strict selected gate passes | Source, fixtures, and strict receipt | Latest pinned report has 1,762 warnings | equivalent | Source findings remain after the declared correction rounds | Regroup before another bounded source pass | active |

## Resumed correction budget

The export batch reduced the original 2,458 findings to 2,320 in the unchanged Nix scope.
Two additional source rounds cover inline standard-library scopes and compiler-reported non-trait imports.
Each round requires helper controls, source review, affected tests, strict Clippy, and a fresh pinned report.
Ambiguous scopes remain unchanged. A smaller count does not establish strict acceptance.

The first inline-module attempt exposed a macro punctuation error in the temporary helper.
A single field colon is not a Rust path separator.
The helper now distinguishes `field: Type::new()` from an existing `owner::Type` path.
Positive and negative controls cover both forms.
The failed source attempt and its logs remain available before regeneration from the verified commit.

The two added source rounds are complete. They reduced the report from 2,320 to 1,814 findings.
The source-correction budget ends with a checked partial result, not strict acceptance.
Residual findings include imports, repeated names, module filenames, and generated BPF imports.
Their correction needs another bounded source pass and fresh strict evidence.

New broad-check evidence justified two narrow build-repair rounds after those source changes.
The isolated adapter needed two existing dependency edges in its generated lockfile.
Crane artifact selection needed the same Cargo metadata correction in both host and musl compositions.
Neither repair changes the VM Cohort revision, compiler, root lockfiles, artifact filters, or lint policy.

The final broad retry now fails at a distinct vendor-resource boundary, not Cargo package-ID formatting.
The preserved result is partial: Cargo compatibility is validated, source findings decreased, and the full change remains blocked.
No accepted-spec sync, archive, or main integration occurs.

## Continued pass

The resumed request adds the bounded vendor pass in `vendor.md` and two source passes in `serde-scopes.md` and `framing.md`.
The private-layout route passes without a Rust patch or shared vendor-root config.
The source passes remove another 52 findings. Their tests and strict Clippy pass.
All pinned policy identifiers and enforcement settings remain unchanged.
The remaining source work is incomplete. The separate SpaceWasm identity mismatch still blocks the broad rail.

