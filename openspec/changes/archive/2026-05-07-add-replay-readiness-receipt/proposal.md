## Why

`nix run .#replay-readiness` is now the single operator button for the Antithesis-alternative replay slice, but dashboards and CI still have to scrape stdout to know which gates ran, which optional dogfood workload was selected, and where failure occurred.

## What Changes

- Add a machine-readable receipt option to the replay readiness operator command.
- Record static gate outcomes, selected dogfood metadata, final status, and failure location in JSON.
- Document the receipt path for dashboard/CI consumers while keeping slow KVM dogfood explicit.

## Impact

- **Files**: `flake.nix`, README/docs, OpenSpec replay-parent-snapshots spec.
- **APIs**: `replay-readiness --receipt <path>`.
- **Testing**: checks-only readiness run with receipt, failure-shape smoke if feasible, existing evidence contract/readiness gates, OpenSpec validation, whitespace check.
