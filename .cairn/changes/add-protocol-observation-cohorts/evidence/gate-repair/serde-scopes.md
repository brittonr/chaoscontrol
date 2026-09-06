# Serde inline-scope repair

## Scope

Continue the earlier derive correction through admitted inline modules.
The syntax helper qualifies Serde derives and retains trait imports until the compiler identifies unused imports.
It rejects ambiguous imports, re-exports, external-module inheritance, conditional attributes, and Serde names inside arbitrary macro tokens.
The helper does not resolve Rust names. Compiler checks and behavior tests remain necessary.

The reviewed proposal selects 34 files in the seven-package source scope.
The patch changes derive paths and imports, not fields, variants, identity strings, or runtime policy.
`serde-scopes-proposals.log` records the selected files and the two rejected files.
`controller.rs` and `scheduler.rs` remain outside this automated correction.

## Controls and baseline

The original helper pin selected a 2023 nightly that cannot parse the current script format.
`serde-scopes-controls.log` retains that setup failure.
The corrected helper pins Fenix `03864c059200a8a96f2ee6bb050c69eae96f57ca`, published on 2026-09-05.
`serde-scopes-controls-current.log` passes the positive, idempotence, shadowing, malformed-input, stale-edit, overlap, and depth controls.

The first baseline waited on the shared Cargo build lock. I stopped only that waiting task.
No test result exists for that interrupted attempt.
The worktree-local retry passes seven-package tests across all targets and all features.
`serde-scopes-isolated-baseline.log` and its exit file retain that baseline.
Later Cargo commands use the same worktree-local target directory.

## Source checks

The initial source pass compiles and passes the seven-package tests.
Strict Clippy then rejects unused trait imports. The first pinned Octet report includes those imports and reports 1,799 warnings.
Neither result establishes a completed source repair.

`cargo fix` removes only compiler-reported unused imports in the selected packages.
`serde-scopes-import-fix.log` records those corrections.
The final seven-package tests and strict Clippy pass across all targets and all features.
The scoped Nix protocol tests and Nickel controls pass.
The pinned Octet report decreases from 1,814 to 1,766 findings, with zero errors and no unused-import findings.
The config hash and profile hash remain unchanged.
`serde-scopes-tests-final.log`, `serde-scopes-clippy-final.log`, and `serde-scopes-nix-final.log` retain those results.
The source batch is complete, but the remaining warning-only result still blocks strict acceptance.
