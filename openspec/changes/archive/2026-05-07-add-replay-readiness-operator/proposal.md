## Why

Replay proof evidence is now split across manifest checks, generated readiness docs, and per-workload accepted-verdict dogfood wrappers. Operators need one bounded command that answers whether the committed replay/evidence slice is ready, and optionally launches one selected proof rail without memorizing the individual scripts.

## What Changes

- Add an operator-facing Nix app/package that runs the existing manifest, evidence, coverage, readiness, and artifact-size checks in sequence.
- Let the same command optionally run exactly one selected accepted-verdict dogfood wrapper (`raft`, `redb`, `net`, or `rust-workload`) after the static readiness checks pass.
- Document the command as the first-line Antithesis-alternative readiness button while preserving the existing scoped anti-claims.

## Impact

- **Files**: `flake.nix`, `README.md`, and replay-parent snapshot OpenSpec materials.
- **APIs**: New Nix app/package `replay-readiness`.
- **Dependencies**: Reuses existing Python/Nickel checks and dogfood wrappers; no new third-party dependency.
- **Testing**: Validate OpenSpec, build/run the checks-only app, build the wrapper package, run the evidence-contracts Nix check, and run whitespace checks.
