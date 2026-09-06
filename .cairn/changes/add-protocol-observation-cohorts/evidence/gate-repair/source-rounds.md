# Source repair after checkpoint 61097cd

## Completion contract

Reduce the remaining source findings without changing protocol meaning, public paths, serialized fields, dependency pins, or enforcement settings.
Strict acceptance requires zero findings in the required Octet scope. A warning-only result does not complete the quality task.
The source starts at `61097cdaac27bf7aa782e4f04b57286e1a9d388f`.
The retained input manifests pass before this round. The worktree starts clean.

The preceding pinned report contains 1,762 findings.
The main groups are 975 non-trait imports, 484 repeated path words, 104 module filenames, and 81 oversized files.
Other groups remain visible in `framing-nix.log`.

## Limits and audit risks

This pass uses serial, correlated review lenses without subagents.
The budget permits three source batches and two correction attempts per batch.
Each batch needs a pre-change test baseline, positive and negative controls, compiler checks, and a new pinned Octet report.
Repository sources and retained reports own the evidence. Helper output does not resolve Rust names or prove behavioral equivalence.
No new runtime component is necessary. This pass reuses the retained syntax helpers before extending their admitted input shapes.
The source and helper bounds stay explicit. Ambiguous scopes remain unchanged.

Public re-exports, conditional imports, child modules, macros, and shadowed names need separate review.
The corrections must preserve strings, wire fields, canonical identities, and control flow.
The primary checkout and other worktrees remain untouched.
The SpaceWasm bundle guard and admitted identities remain unchanged. That route stays blocked without new producer evidence.

## Approach registry

| Family | Mechanism | Evidence | State | Next check |
| --- | --- | --- | --- | --- |
| Existing owner qualifier | Reuse the checked helper after the Serde cleanup | `7ed930b`, `owner-round.md`, 60 fewer findings | Validated bounded result | Retain compiler and negative controls |
| Local namespace admission | Distinguish unrelated imports from conflicting bindings | `4959d75`, `binding-round.md`, 247 fewer findings | Validated bounded result | Review the remaining rejected scopes separately |
| Feature-aware test data | Supply JSON details only for `full` and unit details for the minimal API | `4343e37`, `minimal.md`, 57 minimal cases and target refusals pass | Validated bounded result | Retain both compatible feature modes |

The third batch targets the separate no-default test-harness gap.
The current minimal assertion API takes unit details. The full API takes JSON details.
The tests must use the actual contract for each feature mode without enabling the full API or changing its signatures.
Existing successful and failed input cases must remain in both compatible test scopes.

## Terminal result

All three declared source batches are complete. Each owner batch needed one Rustdoc correction.
The binding audit also rejected an exact token comparison that did not tolerate formatter output.
Reconstruction from the original source and the same formatter passed before the explicit Rustdoc link edits.
The minimal batch used both correction attempts to declare the three intrinsically full-only targets.
These checks do not establish independent review or exhaustive semantic equivalence.

The pinned report decreases from 1,762 to 1,455 findings, with zero errors and unchanged policy hashes.
Seven-package all-feature tests, strict Clippy, strict Rustdoc, scoped formatting, and four bounded KVM cases pass.
The compatible no-default tests and strict Clippy also pass. Explicit incompatible target requests reject.
The strict isolated adapter check passes with zero findings, but it does not replace the broader source gate.

The source-correction budget is exhausted with checked partial progress, not strict acceptance.
The current full flake check rejects the retained SpaceWasm manifest before runtime comparison.
The remaining source work needs another bounded owner-correct pass. SpaceWasm needs compatible producer evidence or explicit review of a new cohort.
The lifecycle change stays active until its quality and publication tasks have complete evidence.
No accepted-spec sync, archive, or main integration occurs.
